//! Walks the sync targets one after another, spacing out the Slack calls to
//! stay inside the rate limit, and refreshes the catalog now and then.

use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::sync::{Mutex, Notify, OwnedMutexGuard};

use crate::db::now_unix;
use crate::slack::api::SlackApi;

use super::{new_batch_id, Pacer, SyncEvent, SyncKind, SyncService};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncStatus {
    pub running: bool,
    pub last_started_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_catalog_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleSummary {
    pub batch_id: String,
    pub total: usize,
    pub failed: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CycleError {
    #[error("a sync is already running")]
    AlreadyRunning,
    #[error("{0}")]
    Failed(String),
}

pub struct SyncScheduler<A> {
    service: SyncService<A>,
    pacer: Pacer,
    cycle_lock: Arc<Mutex<()>>,
    status: Arc<StdMutex<SyncStatus>>,
    wake: Notify,
}

/// Clears `running` even when a cycle is cancelled halfway.
struct RunningGuard(Arc<StdMutex<SyncStatus>>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let mut status = self.0.lock().unwrap();
        status.running = false;
        status.last_finished_at = Some(now_unix());
    }
}

impl<A: SlackApi> SyncScheduler<A> {
    pub fn new(service: SyncService<A>) -> Self {
        let pacer = Pacer::new(service.config().request_spacing);
        Self {
            service,
            pacer,
            cycle_lock: Arc::new(Mutex::new(())),
            status: Arc::new(StdMutex::new(SyncStatus::default())),
            wake: Notify::new(),
        }
    }

    pub fn service(&self) -> &SyncService<A> {
        &self.service
    }

    pub fn status(&self) -> SyncStatus {
        self.status.lock().unwrap().clone()
    }

    /// Ends the wait between two passes early.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// One pass over every sync target. `paced` spaces the Slack calls by the
    /// configured interval. `kind` goes out on the batch's events.
    /// Fails with `AlreadyRunning` while another pass is in progress.
    pub async fn run_cycle(&self, paced: bool, kind: SyncKind) -> Result<CycleSummary, CycleError> {
        let _held = self.try_begin()?;
        self.execute(paced, kind, new_batch_id()).await
    }

    /// Starts a pass in the background and returns its batch id at once. Fails
    /// with `AlreadyRunning` (and starts nothing) while another pass is in progress.
    pub fn spawn_cycle(self: &Arc<Self>, paced: bool, kind: SyncKind) -> Result<String, CycleError>
    where
        A: 'static,
    {
        let held = self.try_begin()?;
        let this = self.clone();
        let batch_id = new_batch_id();
        let id = batch_id.clone();
        tokio::spawn(async move {
            let _held = held;
            if let Err(err) = this.execute(paced, kind, id).await {
                tracing::error!("sync pass failed: {err}");
                this.service.emit(SyncEvent::Error {
                    message: err.to_string(),
                });
            }
        });
        Ok(batch_id)
    }

    /// Takes the single-pass lock and flags the scheduler as running.
    fn try_begin(&self) -> Result<(OwnedMutexGuard<()>, RunningGuard), CycleError> {
        let cycle = self
            .cycle_lock
            .clone()
            .try_lock_owned()
            .map_err(|_| CycleError::AlreadyRunning)?;
        {
            let mut status = self.status.lock().unwrap();
            status.running = true;
            status.last_started_at = Some(now_unix());
        }
        Ok((cycle, RunningGuard(self.status.clone())))
    }

    async fn execute(&self, paced: bool, kind: SyncKind, batch_id: String) -> Result<CycleSummary, CycleError> {
        let targets = self
            .service
            .sync_targets()
            .await
            .map_err(|e| CycleError::Failed(e.to_string()))?;
        let total = targets.len();
        self.service.emit(SyncEvent::Started {
            batch_id: batch_id.clone(),
            kind,
            total,
        });

        let unpaced = Pacer::new(Duration::ZERO);
        let pacer = if paced { &self.pacer } else { &unpaced };
        let mut failed = 0;
        for (index, target) in targets.iter().enumerate() {
            // A failure is already recorded in sync_state and announced as an Error event.
            if self.service.sync_channel_paced(&target.channel_id, pacer).await.is_err() {
                failed += 1;
            }
            self.service.emit(SyncEvent::Progress {
                batch_id: batch_id.clone(),
                kind,
                done: index + 1,
                total,
            });
        }
        self.service.emit(SyncEvent::Completed {
            batch_id: batch_id.clone(),
            kind,
            failed,
        });
        Ok(CycleSummary { batch_id, total, failed })
    }

    /// Re-reads the catalog; remembers when it last succeeded.
    pub async fn refresh_catalog(&self) -> bool {
        let ok = self.service.sync_catalog().await.is_ok();
        if ok {
            self.status.lock().unwrap().last_catalog_at = Some(now_unix());
        }
        ok
    }

    /// The long-running loop: catalog (when due), one paced pass, then wait
    /// `cycle_interval` (or until `wake()`), until `shutdown` completes.
    pub async fn run(&self, shutdown: impl Future<Output = ()>) {
        tokio::pin!(shutdown);
        let config = self.service.config().clone();
        loop {
            let catalog_due = self
                .status()
                .last_catalog_at
                .map(|at| now_unix() - at >= config.catalog_interval.as_secs() as i64)
                .unwrap_or(true);

            let work = async {
                if catalog_due {
                    self.refresh_catalog().await;
                }
                match self.run_cycle(true, SyncKind::Scheduler).await {
                    Ok(summary) => {
                        tracing::info!(total = summary.total, failed = summary.failed, "sync pass finished");
                    }
                    Err(CycleError::AlreadyRunning) => {}
                    Err(err) => {
                        tracing::error!("sync pass failed: {err}");
                        self.service.emit(SyncEvent::Error {
                            message: err.to_string(),
                        });
                    }
                }
            };
            tokio::select! {
                _ = &mut shutdown => return,
                _ = work => {}
            }

            tokio::select! {
                _ = &mut shutdown => return,
                _ = tokio::time::sleep(config.cycle_interval) => {}
                _ = self.wake.notified() => {}
            }
        }
    }
}
