use crate::auth::GoogleUserInfo;
use crate::db::DatabasePool;
use crate::finnhub::fetch_stock_price;
use crate::models::{HoldingResponse, Portfolio, Transaction};
use axum::{extract::State, http::StatusCode, Json};
use tower_sessions::Session;

pub async fn get_portfolio(
    session: Session,
    State(pool): State<DatabasePool>,
) -> Result<(StatusCode, Json<Portfolio>), (StatusCode, Json<String>)> {
    let sess: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    let s = sess.email;

    // validate that this session exists!
    if s.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json("Unauthorized access".to_string()),
        ));
    }

    // Get a database connection
    let conn = pool.0.lock().await;

    // Fetch holdings in a blocking manner
    let holdings: Vec<HoldingResponse> = {
        let mut stmt = match conn
            .prepare("SELECT stock_symbol, stock_name, quantity, purchase_price FROM holdings WHERE account_id = ?")
        {
            Ok(stmt) => stmt,
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json("Failed to prepare SQL statement".to_string()),
                ));
            }
        };

        match stmt
            .query_map([&s], |row| {
                Ok(HoldingResponse {
                    stock_symbol: row.get(0)?,
                    stock_name: row.get(1)?,
                    quantity: row.get(2)?,
                    purchase_price: row.get(3)?,
                    current_price: 0,
                    total_value: 0,
                    day_change: 0,
                    day_change_percent: 0,
                })
            })
            .and_then(|mapped| mapped.collect::<Result<Vec<_>, _>>())
        {
            Ok(holdings) => holdings,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(format!("Failed to fetch holdings: {}", e)),
                ));
            }
        }
    };

    // Asynchronously fetch stock prices
    let mut updated_holdings = Vec::new();

    let mut sum_of_total_values = 0;

    for holding in holdings {
        match fetch_stock_price(&holding.stock_symbol).await {
            Ok(quote) => {
                let price = (quote.c * 100.0) as i32; // Convert to cents
                let day_change = (quote.d * 100.0) as i32;
                let day_change_percent = (quote.dp * 100.0) as i32;

                let total_value = price * holding.quantity;
                sum_of_total_values += total_value;
                updated_holdings.push(HoldingResponse {
                    current_price: price,
                    day_change,
                    day_change_percent,

                    total_value,
                    ..holding
                });
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(format!("Failed to fetch stock price: {}", e)),
                ));
            }
        }
    }

    // Update account value = cash + sum of total values of holdings
    conn.execute(
        "UPDATE accounts SET value = cash + ? WHERE id = ?",
        rusqlite::params![sum_of_total_values, &s],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Failed to update account value: {}", e)),
        )
    })?;

    // Return the portfolio
    Ok((
        StatusCode::OK,
        Json(Portfolio {
            holdings: updated_holdings,
        }),
    ))
}

pub async fn get_transaction_history(
    State(pool): State<DatabasePool>,
    session: Session,
) -> Result<(StatusCode, Json<Vec<Transaction>>), (StatusCode, Json<String>)> {
    let sess: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    let s = sess.email;

    // validate that this session exists!
    if s.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json("Unauthorized access".to_string()),
        ));
    }

    let conn = pool.0.lock().await;

    let mut stmt = conn
        .prepare(
            "SELECT id, stock_symbol, transaction_type, quantity, price, timestamp
             FROM transactions
             WHERE account_id = ?
             ORDER BY timestamp DESC",
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Database error: {}", e)),
            )
        })?;

    let transactions: Vec<Transaction> = stmt
        .query_map([&s], |row| {
            Ok(Transaction {
                id: row.get(0)?,
                account_id: s.clone(),
                stock_symbol: row.get(1)?,
                transaction_type: row.get(2)?,
                quantity: row.get(3)?,
                price: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Database error: {}", e)),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Database error: {}", e)),
            )
        })?;

    Ok((StatusCode::OK, Json(transactions)))
}
