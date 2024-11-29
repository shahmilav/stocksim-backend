mod db;
mod handlers;
mod models;

mod auth;
mod finnhub;

use crate::auth::{get_user_data, handle_google_callback, logout, start_google_login};
use axum::http::header::{ACCESS_CONTROL_ALLOW_CREDENTIALS, CONTENT_TYPE, COOKIE};
use axum::http::HeaderValue;
use axum::{
    routing::{get, post},
    Router,
};
use db::DatabasePool;
use handlers::{
    accounts::get_account,
    portfolio::{get_portfolio, get_transaction_history},
    trading::{buy_stock, sell_stock},
};
use reqwest::Method;
use rusqlite::Connection;
use time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::trace::{self, TraceLayer};
use tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_rusqlite_store::RusqliteStore;
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open(&"sessions.db").unwrap();
    let session_store = RusqliteStore::new(conn.into());
    session_store.migrate().await?;
    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(5)),
    );

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)))
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_http_only(true)
        .with_path("/");

    // Initalize dotenv so we can read .env file
    dotenv::dotenv().ok();

    let cors = CorsLayer::new()
        .allow_credentials(true)
        .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap())
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(vec![ACCESS_CONTROL_ALLOW_CREDENTIALS, CONTENT_TYPE, COOKIE]);

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    // Initialize database pool
    let pool = DatabasePool::new().unwrap();

    // Build application with routes
    let app = Router::new()
        .route("/user", get(get_user_data))
        .route("/logout", get(logout))
        // Account routes
        .route("/account", get(get_account))
        // Trading routes
        .route("/buy", post(buy_stock))
        .route("/sell", post(sell_stock))
        // Portfolio and transaction routes
        .route("/portfolio", get(get_portfolio))
        .route("/transactions", get(get_transaction_history))
        // Auth routes
        .route("/login", get(start_google_login))
        .route("/callback", get(handle_google_callback))
        .with_state(pool)
        .layer(session_layer)
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Listening on: {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();

    deletion_task.await??;

    Ok(())
}
