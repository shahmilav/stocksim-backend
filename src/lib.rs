// src/lib.rs
pub mod db;
pub mod handlers;
pub mod models;

pub mod finnhub;
pub mod auth;

// Re-export commonly used items
pub use db::DatabasePool;
pub use models::*;