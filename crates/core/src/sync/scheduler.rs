//! Walks the sync targets one after another, spacing out the Slack calls to
//! stay inside the rate limit, and refreshes the catalog now and then.

use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::sync::{Mutex, Notify};

use crate::db::now_unix;
use crate::slack::api::SlackApi;

use super::{Pacer, SyncEvent, SyncService};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncStatus {
    pub running: bool,
    pub last_started_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_catalog_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleSummary {
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
    cycle_lock: Mutex<()>,
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
            cycle_lock: Mutex::new(()),
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
    /// configured interval; a manual "sync now" would pass `false`.
    /// Fails with `AlreadyRunning` while another pass is in progress.
    pub async fn run_cycle(&self, paced: bool) -> Result<CycleSummary, CycleError> {
        let Ok(_cycle) = self.cycle_lock.try_lock() else {
            return Err(CycleError::AlreadyRunning);
        };
        {
            let mut status = self.status.lock().unwrap();
            status.running = true;
            status.last_started_at = Some(now_unix());
        }
        let _running = RunningGuard(self.status.clone());

        let targets = self
            .service
            .sync_targets()
            .await
            .map_err(|e| CycleError::Failed(e.to_string()))?;
        let total = targets.len();
        self.service.emit(SyncEvent::Started { total });

        let unpaced = Pacer::new(Duration::ZERO);
        let pacer = if paced { &self.pacer } else { &unpaced };
        let mut failed = 0;
        for (index, target) in targets.iter().enumerate() {
            // A failure is already recorded in sync_state and announced as an Error event.
            if self.service.sync_channel_paced(&target.channel_id, pacer).await.is_err() {
                failed += 1;
            }
            self.service.emit(SyncEvent::Progress {
                done: index + 1,
                total,
            });
        }
        self.service.emit(SyncEvent::Completed { failed });
        Ok(CycleSummary { total, failed })
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
                match self.run_cycle(true).await {
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
