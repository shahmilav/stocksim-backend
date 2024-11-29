use crate::auth::GoogleUserInfo;
use crate::db::DatabasePool;
use crate::finnhub::{fetch_stock_name, fetch_stock_price};
use crate::models::{TradeRequest, Transaction};
use axum::{extract::State, http::StatusCode, Json};
use tower_sessions::Session;

/// Buy a stock with a given account ID. The request body should contain the stock symbol and the quantity to buy.
#[axum::debug_handler]
pub async fn buy_stock(
    State(pool): State<DatabasePool>,
    session: Session,
    Json(trade): Json<TradeRequest>,
) -> Result<(StatusCode, Json<Transaction>), (StatusCode, Json<String>)> {
    let sess: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    let s = sess.email;

    // validate that this session exists!
    if s.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json("Unauthorized access".to_string()),
        ));
    }

    let mut conn = pool.0.lock().await;

    // Fetch stock price from Finnhub API
    let stock_price = (fetch_stock_price(&trade.stock_symbol)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(format!("Error fetching stock price: {}", e)),
            )
        })?
        .c
        * 100.0) as i32;

    let stock_name = fetch_stock_name(&trade.stock_symbol).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(format!("Error fetching stock name: {}", e)),
        )
    })?;

    let total_cost = stock_price * trade.quantity;

    // Start transaction
    let tx = conn.transaction().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    // Check if account has enough cash
    let mut account_cash: i32 = tx
        .query_row("SELECT cash FROM accounts WHERE id = ?", [&s], |row| {
            row.get(0)
        })
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(format!("Account {} not found", s)),
            )
        })?;

    if account_cash < total_cost {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(String::from("Insufficient funds")),
        ));
    }

    // Update account cash
    account_cash -= total_cost;
    tx.execute(
        "UPDATE accounts SET cash = ? WHERE id = ?",
        rusqlite::params![account_cash, &s],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    // Update or insert holding
    tx.execute(
    "INSERT INTO holdings (account_id, stock_symbol, stock_name, quantity, purchase_price)
     VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(account_id, stock_symbol)
     DO UPDATE SET 
         quantity = holdings.quantity + excluded.quantity,
         purchase_price = ((holdings.purchase_price * holdings.quantity) + (excluded.purchase_price * excluded.quantity)) 
                          / (holdings.quantity + excluded.quantity)",
    rusqlite::params![
        &s,
        &trade.stock_symbol,
        stock_name,
        trade.quantity,
        stock_price,
    ],
)
.map_err(|e| {
    tracing::error!("Error inserting holding: {}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(format!("Database error: {}", e)),
    )
})?;

    // Record transaction
    let transaction_id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO transactions (id, account_id, stock_symbol, transaction_type, quantity, price)
         VALUES (?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            &transaction_id,
            &s,
            &trade.stock_symbol,
            "BUY",
            trade.quantity,
            stock_price
        ],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    tx.commit().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    let transaction = Transaction {
        id: transaction_id,
        account_id: s,
        stock_symbol: trade.stock_symbol,
        transaction_type: String::from("BUY"),
        quantity: trade.quantity,
        price: stock_price,
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(transaction)))
}

/// Sell a stock with a given account ID. The request body should contain the stock symbol and the quantity to sell.
pub async fn sell_stock(
    State(pool): State<DatabasePool>,
    session: Session,
    Json(trade): Json<TradeRequest>,
) -> Result<(StatusCode, Json<Transaction>), (StatusCode, Json<String>)> {
    let sess: GoogleUserInfo = session.get("SESSION").await.unwrap().unwrap_or_default();
    let s = sess.email;

    // validate that this session exists!
    if s.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json("Unauthorized access".to_string()),
        ));
    }

    let mut conn = pool.0.lock().await;

    // Fetch stock price from Finnhub API
    let stock_price = (fetch_stock_price(&trade.stock_symbol)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(format!("Error fetching stock price: {}", e)),
            )
        })?
        .c
        * 100.0) as i32;

    let total_value = stock_price * trade.quantity;

    let tx = conn.transaction().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    // Check if account has enough shares
    let current_quantity: i32 = tx
        .query_row(
            "SELECT quantity FROM holdings WHERE account_id = ? AND stock_symbol = ?",
            [&s, &trade.stock_symbol],
            |row| row.get(0),
        )
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(String::from("No shares owned of this stock")),
            )
        })?;

    if current_quantity < trade.quantity {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(String::from("Insufficient shares")),
        ));
    }

    // Update account cash
    tx.execute(
        "UPDATE accounts SET cash = cash + ? WHERE id = ?",
        rusqlite::params![total_value, &s],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    // Update holdings
    let new_quantity = current_quantity - trade.quantity;
    if new_quantity == 0 {
        tx.execute(
            "DELETE FROM holdings WHERE account_id = ? AND stock_symbol = ?",
            [&s, &trade.stock_symbol],
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Database error: {}", e)),
            )
        })?;
    } else {
        tx.execute(
            "UPDATE holdings SET quantity = ? WHERE account_id = ? AND stock_symbol = ?",
            rusqlite::params![new_quantity, &s, &trade.stock_symbol],
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(format!("Database error: {}", e)),
            )
        })?;
    }

    // Record transaction
    let transaction_id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO transactions (id, account_id, stock_symbol, transaction_type, quantity, price)
         VALUES (?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            &transaction_id,
            &s,
            &trade.stock_symbol,
            "SELL",
            trade.quantity,
            stock_price
        ],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    tx.commit().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(format!("Database error: {}", e)),
        )
    })?;

    let transaction = Transaction {
        id: transaction_id,
        account_id: s,
        stock_symbol: trade.stock_symbol,
        transaction_type: String::from("SELL"),
        quantity: trade.quantity,
        price: stock_price,
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(transaction)))
}
