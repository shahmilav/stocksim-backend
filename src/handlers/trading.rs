use crate::auth::validate_session;
use crate::db::DatabasePool;
use crate::finnhub::{fetch_stock_price, fetch_stock_profile};
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
    let info = match validate_session(session).await {
        Ok(info) => info,
        Err(status) => return Err((status, Json("Unauthorized access".to_string()))),
    };
    let s = info.email;

    let stock_price = match fetch_stock_price(&trade.stock_symbol).await {
        Ok(price) => (price.c * 100.0) as i32,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(String::from("Error completing trade")),
            ))
        }
    };

    let stock_name = match fetch_stock_profile(&trade.stock_symbol).await {
        Ok(stock) => stock.name,
        Err(e) => {
            tracing::error!("Error fetching stock profile: {}", e);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(String::from("Error completing trade")),
            ));
        }
    };

    let total_cost = stock_price * trade.quantity;

    let mut session = pool.client.start_session().await.unwrap();

    session.start_transaction().await.map_err(|e| {
        tracing::error!("Error starting transaction: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(String::from("Error completing trade")),
        )
    })?;

    let result = async {
        // Check if account has enough cash
        // Update account cash
        // Update or insert holding
        // Record transaction
        // Commit transaction
        // Return transaction

        let mut account = pool
            .get_account(&s)
            .await
            .map_err(|e| {
                tracing::error!("Error fetching account: {}", e);
                return Err::<Transaction, (StatusCode, Json<String>)>((
                    StatusCode::NOT_FOUND,
                    Json(String::from("Error completing trade")),
                ));
            })
            .unwrap()
            .unwrap();

        if account.cash < total_cost {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(String::from(
                    "You don't have enough cash to complete this trade.",
                )),
            ));
        }

        account.cash -= total_cost;

        pool.update_account(&s, account.value as i64, account.cash as i64)
            .await
            .map_err(|e| {
                tracing::error!("Error updating account cash: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(String::from("Error completing trade")),
                )
            })?;
        // update holdings
        let holding = pool.get_holding(&s, &trade.stock_symbol).await.unwrap();
        let holding = holding.unwrap_or_default();
        if holding.quantity > 0 {
            let new_quantity = holding.quantity + trade.quantity;
            let new_price = ((holding.purchase_price * holding.quantity)
                + (stock_price * trade.quantity))
                / (holding.quantity + trade.quantity);

            pool.update_holding(
                &s,
                &trade.stock_symbol,
                new_quantity as i64,
                new_price as i64,
            )
            .await
            .map_err(|e| {
                tracing::error!("Error updating holding: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(String::from("Error completing trade")),
                )
            })?;
        } else {
            // insert holding
            pool.add_holding(crate::models::Holding {
                account_id: s.clone(),
                stock_symbol: trade.stock_symbol.clone(),
                stock_name: stock_name.clone(),
                quantity: trade.quantity,
                purchase_price: stock_price,
                total_value: stock_price * trade.quantity,
                current_price: stock_price,
            })
            .await
            .unwrap();
        }

        // Record transaction
        let transaction_id = uuid::Uuid::new_v4().to_string();
        pool.add_transaction(crate::models::Transaction {
            id: transaction_id.clone(),
            account_id: s.clone(),
            stock_symbol: trade.stock_symbol.clone(),
            transaction_type: String::from("BUY"),
            quantity: trade.quantity,
            price: stock_price,
            timestamp: chrono::Local::now().to_rfc3339(),
        })
        .await
        .unwrap();

        Ok(Transaction {
            id: transaction_id,
            account_id: s,
            stock_symbol: trade.stock_symbol,
            transaction_type: String::from("BUY"),
            quantity: trade.quantity,
            price: stock_price,
            timestamp: chrono::Local::now().to_rfc3339(),
        })
    }
    .await;

    match result {
        Ok(transaction) => {
            session.commit_transaction().await.unwrap();
            Ok((StatusCode::CREATED, Json(transaction)))
        }
        Err(e) => {
            session.abort_transaction().await.unwrap();
            Err(e)
        }
    }
}

/// Sell a stock with a given account ID. The request body should contain the stock symbol and the quantity to sell.
pub async fn sell_stock(
    State(pool): State<DatabasePool>,
    session: Session,
    Json(trade): Json<TradeRequest>,
) -> Result<(StatusCode, Json<Transaction>), (StatusCode, Json<String>)> {
    let info = match validate_session(session).await {
        Ok(info) => info,
        Err(status) => return Err((status, Json("Unauthorized access".to_string()))),
    };
    let s = info.email;

    // Fetch stock price from Finnhub API
    let stock_price = (fetch_stock_price(&trade.stock_symbol)
        .await
        .map_err(|e| {
            tracing::error!("Error fetching stock price: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(String::from("Error completing trade")),
            )
        })?
        .c
        * 100.0) as i32;

    let total_value = stock_price * trade.quantity;

    let mut session = pool.client.start_session().await.unwrap();

    session.start_transaction().await.map_err(|e| {
        tracing::error!("Error starting transaction: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(String::from("Error completing trade")),
        )
    })?;

    let result = async {
        // Check if account has enough shares
        // Update account cash
        // Update holdings
        // Record transaction
        // Commit transaction
        // Return transaction

        let mut account = pool
            .get_account(&s)
            .await
            .map_err(|e| {
                tracing::error!("Error fetching account: {}", e);
                return Err::<Transaction, (StatusCode, Json<String>)>((
                    StatusCode::NOT_FOUND,
                    Json(String::from("Error completing trade")),
                ));
            })
            .unwrap()
            .unwrap();

        let current_quantity = pool
            .get_holding(&s, &trade.stock_symbol)
            .await
            .map_err(|e| {
                tracing::error!("Error fetching holding: {}", e);
                return Err::<Transaction, (StatusCode, Json<String>)>((
                    StatusCode::NOT_FOUND,
                    Json(String::from("You cannot sell a stock you do not own.")),
                ));
            })
            .unwrap()
            .unwrap()
            .quantity;

        if current_quantity < trade.quantity {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(String::from("You cannot sell more shares than you own.")),
            ));
        }

        account.cash += total_value;
        pool.update_account(&s, account.value as i64, account.cash as i64)
            .await
            .unwrap();

        let new_quantity = current_quantity - trade.quantity;
        if new_quantity == 0 {
            pool.delete_holding(&s, &trade.stock_symbol).await.unwrap();
        } else {
            let holding = pool
                .get_holding(&s, &trade.stock_symbol)
                .await
                .unwrap()
                .unwrap();
            pool.update_holding(
                &s,
                &trade.stock_symbol,
                new_quantity as i64,
                holding.purchase_price as i64,
            )
            .await
            .unwrap();
        }

        let transaction_id = uuid::Uuid::new_v4().to_string();
        pool.add_transaction(crate::models::Transaction {
            id: transaction_id.clone(),
            account_id: s.clone(),
            stock_symbol: trade.stock_symbol.clone(),
            transaction_type: String::from("SELL"),
            quantity: trade.quantity,
            price: stock_price,
            timestamp: chrono::Local::now().to_rfc3339(),
        })
        .await
        .unwrap();

        Ok(Transaction {
            id: transaction_id,
            account_id: s,
            stock_symbol: trade.stock_symbol,
            transaction_type: String::from("SELL"),
            quantity: trade.quantity,
            price: stock_price,
            timestamp: chrono::Local::now().to_rfc3339(),
        })
    }
    .await;

    match result {
        Ok(transaction) => {
            session.commit_transaction().await.unwrap();
            Ok((StatusCode::CREATED, Json(transaction)))
        }
        Err(e) => {
            session.abort_transaction().await.unwrap();
            Err(e)
        }
    }
}
