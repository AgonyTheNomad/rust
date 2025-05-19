// src/influx.rs
use crate::models::Candle;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::collections::HashMap;
use std::error::Error;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfluxConfig {
    pub url: String,
    pub token: String,
    pub org: String,
    pub bucket: String,
}

impl InfluxConfig {
    pub fn new(url: &str, token: &str, org: &str, bucket: &str) -> Self {
        Self {
            url: url.to_string(),
            token: token.to_string(),
            org: org.to_string(),
            bucket: bucket.to_string(),
        }
    }
    
    pub fn default() -> Self {
        Self {
            url: "http://0.0.0.0:8086".to_string(),
            token: "Xu0vYUoLT_lAA02JKERHPS5jl02cN4YA76AJzZMH7FeApVKksrrcafLm3WVcZJj6VcZm53oUgR6PE8HMq39IpQ==".to_string(),
            org: "ValhallaVault".to_string(),
            bucket: "hyper_candles".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct FluxQuery {
    query: String,
}

pub async fn get_candles(config: &InfluxConfig, symbol: &str, start_time: Option<&str>) -> Result<Vec<Candle>, Box<dyn Error>> {
    println!("Connecting to InfluxDB at {}", config.url);
    
    // Configure HTTP client with appropriate timeouts
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
        
    // Define start time range
    let start_range = match start_time {
        Some(time) => format!("start: {}", time),
        None => "start: -365d".to_string(),
    };
    
    // Construct the Flux query
    let flux_query = format!(
        r#"
        from(bucket: "{}")
            |> range({})
            |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "{}")
            |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")
        "#,
        config.bucket, start_range, symbol
    );

    println!("Executing InfluxDB Flux query: {}", flux_query);

    // Prepare request to InfluxDB API v2
    let api_url = format!("{}/api/v2/query", config.url);
    
    let response = client
        .post(&api_url)
        .header("Authorization", format!("Token {}", config.token))
        .header("Content-Type", "application/json")
        .header("Accept", "application/csv")
        .query(&[("org", &config.org)])
        .body(serde_json::to_string(&FluxQuery { query: flux_query })?)
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        let status = response.status();
        let error_text = response.text().await?;
        return Err(format!("InfluxDB query failed with status {}: {}", status, error_text).into());
    }

    // Process CSV response
    let csv_text = response.text().await?;
    let mut candles = Vec::new();
    
    // Parse CSV
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());

    for result in rdr.deserialize() {
        let record: HashMap<String, String> = result?;
        
        // Skip header rows or other metadata
        if !record.contains_key("_time") || !record.contains_key("open") {
            continue;
        }
        
        // Extract fields with error handling
        if let Some(time_str) = record.get("_time") {
            // Extract price and volume data with fallbacks for missing fields
            let open: f64 = record.get("open").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let high: f64 = record.get("high").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0); 
            let low: f64 = record.get("low").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let close: f64 = record.get("close").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let volume: f64 = record.get("volume").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let num_trades: i64 = record.get("num_trades").unwrap_or(&"0".to_string()).parse().unwrap_or(0);
            
            // Create a Candle object
            let candle = Candle {
                time: time_str.clone(),
                open,
                high,
                low,
                close,
                volume,
                num_trades,
            };
            
            candles.push(candle);
        }
    }

    // Sort by time to ensure consistent ordering
    candles.sort_by(|a, b| a.time.cmp(&b.time));
    
    println!("Loaded {} candles from InfluxDB", candles.len());
    
    Ok(candles)
}

pub async fn get_available_symbols(config: &InfluxConfig) -> Result<Vec<String>, Box<dyn Error>> {
    // Configure HTTP client with appropriate timeouts
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    
    // Construct a Flux query to get distinct symbols
    let flux_query = format!(
        r#"
        import "influxdata/influxdb/schema"
        schema.measurementTagValues(
            bucket: "{}",
            measurement: "candles",
            tag: "symbol"
        )
        "#,
        config.bucket
    );

    println!("Executing symbols query");

    // Prepare request to InfluxDB API v2
    let api_url = format!("{}/api/v2/query", config.url);
    
    let response = client
        .post(&api_url)
        .header("Authorization", format!("Token {}", config.token))
        .header("Content-Type", "application/json")
        .header("Accept", "application/csv")
        .query(&[("org", &config.org)])
        .body(serde_json::to_string(&FluxQuery { query: flux_query })?)
        .send()
        .await?;

    if response.status() != StatusCode::OK {
        let status = response.status();
        let error_text = response.text().await?;
        return Err(format!("InfluxDB symbols query failed with status {}: {}", status, error_text).into());
    }

    // Process CSV response
    let csv_text = response.text().await?;
    let mut symbols = Vec::new();
    
    // Parse CSV
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());

    for result in rdr.deserialize() {
        let record: HashMap<String, String> = result?;
        
        // Extract value field (contains the symbol)
        if let Some(symbol) = record.get("_value") {
            symbols.push(symbol.clone());
        }
    }

    println!("Found {} symbols in InfluxDB", symbols.len());
    Ok(symbols)
}