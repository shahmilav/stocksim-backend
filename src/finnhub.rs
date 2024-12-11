use reqwest;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Response structure for Finnhub API
#[derive(Deserialize, Clone)]
pub struct FinnhubQuote {
    pub c: f64,  // Current price
    pub d: f64,  // Day change
    pub dp: f64, // Day change percentage
    pub pc: f64, // Previous close
}

/// Response structure for Finnhub API
#[derive(Deserialize, Clone)]
pub struct FinnhubProfile {
    pub name: String,
    pub logo: String,
    #[serde(rename = "finnhubIndustry")]
    pub finnhub_industry: String,
}

// Make the client and cache static and reusable
lazy_static::lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
    static ref CACHE: Mutex<HashMap<String, (FinnhubQuote, Instant)>> = Mutex::new(HashMap::new());
    static ref PROFILE_CACHE: Mutex<HashMap<String, (FinnhubProfile, Instant)>> = Mutex::new(HashMap::new());
}

/// Fetch stock profile from Finnhub API. A stock profile includes the name and logo of the company.
pub async fn fetch_stock_profile(symbol: &str) -> Result<FinnhubProfile, String> {
    let api_key = env::var("FINNHUB_API_KEY").expect("Missing FINNHUB_API_KEY");
    let now = Instant::now();

    let mut cache = PROFILE_CACHE.lock().await;
    if let Some((profile, timestamp)) = cache.get(symbol) {
        // Check if the profile is still valid (less than 24 hours)
        if now.duration_since(*timestamp) < Duration::from_secs(60 * 60 * 24) {
            tracing::debug!("Returning cached profile for {}", symbol);
            return Ok(profile.clone());
        }
    }

    let url = format!(
        "https://finnhub.io/api/v1/stock/profile2?symbol={}&token={}",
        symbol, api_key
    );
    let response = CLIENT.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch stock name: HTTP {}",
            response.status()
        ));
    }
    tracing::debug!("Fetched stock profile for {}", symbol);
    let profile: FinnhubProfile = response.json().await.map_err(|e| e.to_string())?;

    cache.insert(symbol.to_string(), (profile.clone(), now));

    Ok(profile)
}

pub async fn fetch_stock_price(symbol: &str) -> Result<FinnhubQuote, String> {
    let api_key = env::var("FINNHUB_API_KEY").expect("Missing FINNHUB_API_KEY");
    let now = Instant::now();

    // Lock the cache using `tokio::sync::Mutex`
    let mut cache = CACHE.lock().await;

    // Check if the symbol is in the cache and still valid
    if let Some((quote, timestamp)) = cache.get(symbol) {
        if now.duration_since(*timestamp) < Duration::from_secs(300) {
            tracing::debug!("Returning cached price for {}", symbol);
            return Ok(quote.clone());
        }
    }

    // Fetch from API if not in cache or expired
    let url = format!(
        "https://finnhub.io/api/v1/quote?symbol={}&token={}",
        symbol, api_key
    );

    let response = CLIENT.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch stock price: HTTP {}",
            response.status()
        ));
    }
    tracing::debug!("Fetched stock price for {}", symbol);

    let quote: FinnhubQuote = response.json().await.map_err(|e| e.to_string())?;
    if quote.c <= 0.0 {
        return Err("Invalid stock price returned".to_string());
    }

    // Update the cache
    cache.insert(symbol.to_string(), (quote.clone(), now));

    Ok(quote)
}
