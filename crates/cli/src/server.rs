use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use rsdiffy_gitlab::GitLabRemote;

use crate::assets::static_handler;
use crate::db;
use crate::routes;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    /// Base git diff arguments (e.g. merge-base..HEAD).
    pub diff_args: Vec<String>,
    /// Human-readable description (e.g. "Changes from main").
    pub description: String,
    /// The effective ref being viewed (e.g. "main", "work", "staged").
    pub effective_ref: String,
    /// Whether to include untracked files in the diff.
    pub include_untracked: bool,
    /// Path to the rsdiffy data directory for this repo.
    pub rsdiffy_dir: String,
    /// Detected GitLab remote, if any.
    pub gitlab_remote: Option<GitLabRemote>,
    /// Available editor ("vscode" or None).
    pub editor_available: Option<String>,
}

pub struct ServerOptions {
    pub port: u16,
    pub port_is_explicit: bool,
    pub diff_args: Vec<String>,
    pub description: String,
    pub effective_ref: String,
    pub rsdiffy_dir: String,
}

pub struct ServerHandle {
    pub port: u16,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl ServerHandle {
    pub fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
    }
}

pub async fn start_server(opts: ServerOptions) -> Result<ServerHandle> {
    let include_untracked = opts.diff_args.is_empty();

    let gitlab_remote = rsdiffy_gitlab::detect_remote().ok();
    let editor_available = detect_editor();
    db::init_db_path(&opts.rsdiffy_dir);

    let state = Arc::new(AppState {
        diff_args: opts.diff_args,
        description: opts.description,
        effective_ref: opts.effective_ref,
        include_untracked,
        rsdiffy_dir: opts.rsdiffy_dir,
        gitlab_remote,
        editor_available,
    });

    let app = Router::new()
        .merge(routes::api_routes())
        .fallback(static_handler)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], opts.port));
    let listener = if opts.port_is_explicit {
        TcpListener::bind(addr).await?
    } else {
        // Try the requested port, then increment up to 10 times
        bind_with_fallback(opts.port, 10).await?
    };

    let actual_port = listener.local_addr()?.port();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok(ServerHandle {
        port: actual_port,
        shutdown_tx,
    })
}

async fn bind_with_fallback(start_port: u16, max_retries: u16) -> Result<TcpListener> {
    for i in 0..=max_retries {
        let port = start_port + i;
        match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await {
            Ok(listener) => return Ok(listener),
            Err(_) if i < max_retries => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

fn detect_editor() -> Option<String> {
    std::process::Command::new("which")
        .arg("code")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| "vscode".to_string())
}
