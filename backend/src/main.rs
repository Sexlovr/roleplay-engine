//! Roleplay Engine backend — axum HTTP server with SQLite storage and LLM proxy.
//!
//! Serves the REST API on `/api/*` and the static Leptos/WASM frontend from
//! `${STATIC_DIR}` (default `./dist`) with SPA fallback.

mod db;
mod error;
mod llm;
mod routes;
mod state;

use axum::{http::StatusCode, routing::get, Router};
use state::AppState;
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Resolve writable data directory.
    let (data_dir, persistent) = db::resolve_data_dir();
    let db_path = std::path::Path::new(&data_dir).join("roleplay.db");
    tracing::info!(data_dir = %data_dir, persistent, db_path = %db_path.display(),
        "data directory resolved");

    // Init pool and run migrations.
    let pool = match db::init_pool(db_path.to_str().unwrap_or("roleplay.db")) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("failed to init database pool: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("database pool ready");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("reqwest client");

    let state = AppState {
        pool,
        http,
        data_dir,
        persistent,
    };

    // Static file serving.
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./dist".into());
    let index_html = std::path::Path::new(&static_dir).join("index.html");
    if !index_html.exists() {
        tracing::warn!(
            "STATIC_DIR={static_dir:?} index.html not found — SPA routes will 404"
        );
    } else {
        tracing::info!("static dir: {static_dir}");
    }

    let serve_dir = ServeDir::new(&static_dir)
        .precompressed_gzip()
        .precompressed_br()
        .fallback(ServeFile::new(index_html));

    let app = Router::new()
        .route("/api/health", get(routes::health::health))
        .route(
            "/api/characters",
            get(routes::characters::list).post(routes::characters::create),
        )
        .route(
            "/api/characters/{id}",
            get(routes::characters::get_one)
                .put(routes::characters::update)
                .delete(routes::characters::delete_one),
        )
        .route(
            "/api/characters/{id}/chats",
            get(routes::chats::list_for_character).post(routes::chats::create_chat),
        )
        .route(
            "/api/chats/{id}",
            get(routes::chats::get_chat).delete(routes::chats::delete_chat),
        )
        .route("/api/chats/{id}/memory", axum::routing::put(routes::chats::update_memory))
        .route("/api/chats/{id}/send", axum::routing::post(routes::chats::send))
        .route(
            "/api/chats/{id}/regenerate",
            axum::routing::post(routes::chats::regenerate),
        )
        .route(
            "/api/messages/{id}",
            axum::routing::put(routes::messages::edit).delete(routes::messages::delete),
        )
        .route(
            "/api/settings",
            get(routes::settings::get_settings).put(routes::settings::put_settings),
        )
        // Health-check helper that always returns OK (useful for reverse proxies).
        .route("/healthz", get(|| async { StatusCode::OK }))
        .with_state(state)
        .fallback_service(serve_dir);

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "7860".into())
        .parse()
        .unwrap_or(7860);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
