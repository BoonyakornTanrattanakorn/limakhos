//! Limakhos self-hosted multiplayer server binary.

use makhos_server::{build_router, db::Database, AppState};
use makhos_tb::TablebaseSet;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("makhos_server=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let bind_addr = std::env::var("LIMAKHOS_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let db_path = std::env::var("LIMAKHOS_DB").unwrap_or_else(|_| "data/limakhos.db".to_string());
    let static_dir =
        std::env::var("LIMAKHOS_STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let tb_dir =
        std::env::var("LIMAKHOS_TB_DIR").unwrap_or_else(|_| "data/tablebases".to_string());

    let db = Database::open_file(&db_path).expect("failed to initialize SQLite database");
    let tb = match TablebaseSet::load_dir(Path::new(&tb_dir)) {
        Ok(set) => {
            if !set.is_empty() {
                tracing::info!(
                    tb_dir = %tb_dir,
                    slices = set.slice_count(),
                    max_pieces = set.max_pieces(),
                    "Loaded offline endgame tablebases"
                );
            }
            set
        }
        Err(err) => {
            tracing::warn!(tb_dir = %tb_dir, error = %err, "Failed to load tablebases; continuing without them");
            TablebaseSet::new()
        }
    };

    let state = AppState::with_tablebases(db, tb);
    let app = build_router(state, Some(PathBuf::from(&static_dir)));

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");
    tracing::info!(
        bind = %bind_addr,
        db = %db_path,
        static_dir = %static_dir,
        "Limakhos server listening"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Shutting down Limakhos server");
}
