//! Shared, cheaply clonable state handed to every request handler.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Semaphore;

use slacksitter_core::db::SqlitePool;
use slacksitter_core::slack::api::SlackApi;
use slacksitter_core::slack::models::AuthResult;
use slacksitter_core::sync::{SyncScheduler, SyncService};

use crate::config::Secret;
use crate::images::ImageProxy;

/// Who the server is signed in to Slack as (from `auth.test`, read once at startup).
#[derive(Debug, Clone)]
pub struct Me {
    pub auth: AuthResult,
}

pub struct AppState<A> {
    inner: Arc<Inner<A>>,
}

struct Inner<A> {
    api_key: Secret,
    scheduler: Arc<SyncScheduler<A>>,
    me: Me,
    images: ImageProxy,
    log_dir: PathBuf,
    manual_sync: Arc<Semaphore>,
}

impl<A> Clone for AppState<A> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<A: SlackApi> AppState<A> {
    pub fn new(
        api_key: Secret,
        scheduler: Arc<SyncScheduler<A>>,
        me: Me,
        images: ImageProxy,
        log_dir: PathBuf,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                api_key,
                scheduler,
                me,
                images,
                log_dir,
                manual_sync: Arc::new(Semaphore::new(1)),
            }),
        }
    }

    pub fn api_key(&self) -> &Secret {
        &self.inner.api_key
    }

    pub fn scheduler(&self) -> &Arc<SyncScheduler<A>> {
        &self.inner.scheduler
    }

    pub fn service(&self) -> &SyncService<A> {
        self.inner.scheduler.service()
    }

    pub fn pool(&self) -> &SqlitePool {
        self.service().pool()
    }

    pub fn me(&self) -> &Me {
        &self.inner.me
    }

    pub fn images(&self) -> &ImageProxy {
        &self.inner.images
    }

    pub fn log_dir(&self) -> &PathBuf {
        &self.inner.log_dir
    }

    /// One permit: batches of channel syncs requested through the API run one at a time,
    /// so refreshing a whole board never hits Slack with every channel at once.
    pub fn manual_sync(&self) -> Arc<Semaphore> {
        self.inner.manual_sync.clone()
    }

    pub fn channel_prefix(&self) -> &str {
        &self.service().config().channel_prefix
    }
}
