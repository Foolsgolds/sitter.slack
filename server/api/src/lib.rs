//! `slacksitter-server`: keeps a SQLite copy of Slack up to date and serves it
//! (plus a few write-through calls) over a small authenticated Web API.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, HeaderValue, Method};
use slacksitter_core::db::{self, view};
use slacksitter_core::slack::client::SlackClient;
use slacksitter_core::slack::models::AuthResult;
use slacksitter_core::sync::{SyncScheduler, SyncService};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub mod auth;
pub mod config;
pub mod error;
pub mod images;
pub mod logging;
pub mod routes;
pub mod state;

use config::Config;
use images::ImageProxy;
use state::{AppState, Me};

/// How long open connections (SSE streams never end by themselves) get to
/// finish after a shutdown request.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub fn build_cors(origins: &[String]) -> Result<CorsLayer, String> {
    let origins = origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin.trim()).map_err(|_| format!("server.cors_origins: invalid origin {origin:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(3600)))
}

/// Startup order: database + migrations -> `auth.test` -> catalog sync ->
/// listen, with the scheduler running in the background. Returns after Ctrl+C.
pub async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let cors = build_cors(&config.server.cors_origins)?;

    let pool = db::connect(&config.database.path)
        .await
        .map_err(|e| format!("cannot open the database {}: {e}", config.database.path.display()))?;
    tracing::info!(path = %config.database.path.display(), "database ready");

    let client = match &config.slack.api_base_url {
        Some(base) => {
            tracing::warn!(%base, "using a custom Slack API base URL (testing only)");
            SlackClient::with_base_url(config.slack.token.expose(), base.clone())
        }
        None => SlackClient::new(config.slack.token.expose()),
    };
    let (workspace_url, user_id) = client
        .auth_test()
        .await
        .map_err(|e| format!("Slack rejected the token (auth.test): {e}"))?;
    tracing::info!(%user_id, "signed in to Slack");

    let service = SyncService::new(pool.clone(), Arc::new(client), config.sync_config());
    if let Err(err) = service.fetch_user(&user_id).await {
        tracing::warn!("could not read the signed-in user's profile: {err}");
    }
    let avatar = view::user_profile(&pool, &user_id)
        .await?
        .map(|profile| profile.image_url)
        .filter(|url| !url.is_empty());
    let me = Me {
        auth: AuthResult {
            workspace_url,
            user_id,
            current_user_avatar_uri: avatar,
        },
    };

    let scheduler = Arc::new(SyncScheduler::new(service));
    if !scheduler.refresh_catalog().await {
        tracing::warn!("the first catalog sync failed; serving what is already in the database");
    }

    let state = AppState::new(
        config.server.api_key.clone(),
        scheduler.clone(),
        me,
        ImageProxy::new(config.slack.token.clone()),
        config.log.dir.clone(),
    );
    let app = routes::router(state, cors);

    let addr = config.bind_addr();
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
    tracing::info!(%addr, "listening");

    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn({
        let stop_tx = stop_tx.clone();
        async move {
            shutdown_signal().await;
            tracing::info!("shutting down");
            let _ = stop_tx.send(true);
        }
    });
    let scheduler_task = tokio::spawn({
        let scheduler = scheduler.clone();
        let mut stop = stop_rx.clone();
        async move {
            scheduler
                .run(async move {
                    let _ = stop.wait_for(|stopped| *stopped).await;
                })
                .await
        }
    });

    let server = axum::serve(listener, app).with_graceful_shutdown({
        let mut stop = stop_rx.clone();
        async move {
            let _ = stop.wait_for(|stopped| *stopped).await;
        }
    });
    let mut stop = stop_rx;
    tokio::select! {
        result = server => result?,
        _ = async {
            let _ = stop.wait_for(|stopped| *stopped).await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => tracing::warn!("open connections did not close in time; exiting anyway"),
    }

    let _ = stop_tx.send(true);
    let _ = scheduler_task.await;
    tracing::info!("stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await;
}
