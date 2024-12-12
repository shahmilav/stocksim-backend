mod auth;
mod db;
mod finnhub;
mod handlers;
mod models;

use crate::auth::{get_user_data, handle_google_callback, logout, start_google_login};
use crate::db::DatabasePool;
use crate::handlers::{
    accounts::get_account,
    portfolio::{get_portfolio, get_transaction_history},
    trading::{buy_stock, sell_stock},
};
use axum::http::header::{ACCESS_CONTROL_ALLOW_CREDENTIALS, CONTENT_TYPE, COOKIE};
use axum::http::HeaderValue;
use axum::{
    routing::{get, post},
    Router,
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
    // Set the log level based on the first argument
    let args: Vec<String> = std::env::args().collect();
    let mut log_level = Level::INFO;
    if args.len() >= 2 {
        log_level = match args[1].as_str() {
            "debug" => Level::DEBUG,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO,
        };
    }

    let db_path = ".";

    // Initialize our session store as a SQLite database
    let conn = Connection::open(format!("{}{}", db_path, "/sessions.db")).unwrap();
    let session_store = RusqliteStore::new(conn.into());
    session_store.migrate().await?;

    // Start a task to delete expired sessions every 5 seconds
    let deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(5)),
    );

    // Create session layer with some configuration
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)))
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_http_only(true)
        .with_path("/");

    // Initalize dotenv so we can read .env file
    dotenv::dotenv().ok();

    let frontend_port =
        dotenv::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let origin = format!("{}", frontend_port);

    // Initialize CORS layer
    let cors = CorsLayer::new()
        .allow_credentials(true)
        .allow_origin(origin.parse::<HeaderValue>().unwrap())
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(vec![ACCESS_CONTROL_ALLOW_CREDENTIALS, CONTENT_TYPE, COOKIE]);

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .with_max_level(log_level)
        .init();

    tracing::info!("Log level set to: {}", log_level);

    let uri = dotenv::var("MONGO_URI").expect("MONGO_URI must be set");
    // Initialize database pool
    let pool = DatabasePool::new(&uri.to_string()).await.unwrap();

    // Build application with routes
    let app = Router::new()
        // Account routes
        .route("/account", get(get_account))
        // Trading routes
        .route("/buy", post(buy_stock))
        .route("/sell", post(sell_stock))
        .route("/portfolio", get(get_portfolio))
        .route("/transactions", get(get_transaction_history))
        // Auth routes
        .route("/login", get(start_google_login))
        .route("/logout", get(logout))
        .route("/callback", get(handle_google_callback))
        .route("/user", get(get_user_data))
        // Database app state
        .with_state(pool)
        // Session, CORS, and tracing layers
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

