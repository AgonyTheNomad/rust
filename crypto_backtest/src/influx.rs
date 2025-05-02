// src/influx.rs
use crate::models::Candle;
use influxdb::{Client, ReadQuery};
use chrono::{DateTime, Utc};
use std::error::Error;
use serde_json::Value;

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
            url: "http://192.168.68.52:30086".to_string(),
            token: "gqRGGfpdf2EAj9Yu-ISLY5QFoFtY4HyYXm_EgT8ywVD_r0A49f5TttA4dikcFlXOGYVzRL5V3mVrWJlQYDF5qw==".to_string(),
            org: "ValhallaVault".to_string(),
            bucket: "hyper_candles".to_string(),
        }
    }
}

pub async fn get_candles(config: &InfluxConfig, symbol: &str, start_time: Option<&str>) -> Result<Vec<Candle>, Box<dyn Error>> {
    println!("Connecting to InfluxDB at {}", config.url);
    
    // Since we're using the older influxdb crate, we'll use InfluxQL instead of Flux
    let time_range = match start_time {
        Some(time) => format!("time > '{}'", time),
        None => format!("time > now() - 365d"),
    };
    
    let query_string = format!(
        "SELECT time, open, high, low, close, volume, num_trades FROM {} WHERE symbol='{}' AND {}",
        config.bucket,
        symbol,
        time_range
    );
    
    println!("Executing query: {}", query_string);
    
    let client = Client::new(config.url.clone(), config.bucket.clone());
    let read_query = ReadQuery::new(query_string);
    
    // Execute the query - the influxdb crate returns a String, not a complex type
    let query_result_string = client.query(&read_query).await?;
    
    // Parse the result as JSON
    let query_result_json: Value = serde_json::from_str(&query_result_string)?;
    
    let mut candles = Vec::new();
    
    // Parse the JSON results assuming it follows the standard InfluxDB JSON format
    if let Some(series_array) = query_result_json.get("series").and_then(|s| s.as_array()) {
        for series in series_array {
            let columns = series.get("columns")
                .and_then(|c| c.as_array())
                .ok_or("No columns found")?;
            
            let column_names: Vec<String> = columns.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect();
            
            let time_idx = column_names.iter().position(|c| c == "time").ok_or("No time column")?;
            let open_idx = column_names.iter().position(|c| c == "open").ok_or("No open column")?;
            let high_idx = column_names.iter().position(|c| c == "high").ok_or("No high column")?;
            let low_idx = column_names.iter().position(|c| c == "low").ok_or("No low column")?;
            let close_idx = column_names.iter().position(|c| c == "close").ok_or("No close column")?;
            let volume_idx = column_names.iter().position(|c| c == "volume").ok_or("No volume column")?;
            let num_trades_idx = column_names.iter().position(|c| c == "num_trades").unwrap_or(6);
            
            if let Some(values_array) = series.get("values").and_then(|v| v.as_array()) {
                for values in values_array {
                    if let Some(values_vec) = values.as_array() {
                        let time_str = match &values_vec[time_idx] {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            _ => continue,
                        };
                        
                        // Parse timestamp
                        let time_parsed = if let Ok(dt) = time_str.parse::<DateTime<Utc>>() {
                            dt.to_rfc3339()
                        } else {
                            time_str
                        };
                        
                        let open = values_vec[open_idx].as_f64().unwrap_or(0.0);
                        let high = values_vec[high_idx].as_f64().unwrap_or(0.0);
                        let low = values_vec[low_idx].as_f64().unwrap_or(0.0);
                        let close = values_vec[close_idx].as_f64().unwrap_or(0.0);
                        let volume = values_vec[volume_idx].as_f64().unwrap_or(0.0);
                        let num_trades = values_vec.get(num_trades_idx)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        
                        candles.push(Candle {
                            time: time_parsed,
                            open,
                            high,
                            low,
                            close,
                            volume,
                            num_trades,
                        });
                    }
                }
            }
        }
    }
    
    // Sort candles by time
    candles.sort_by(|a, b| a.time.cmp(&b.time));
    
    println!("Loaded {} candles from InfluxDB", candles.len());
    
    Ok(candles)
}

// Function to get all available symbols from the InfluxDB
pub async fn get_available_symbols(config: &InfluxConfig) -> Result<Vec<String>, Box<dyn Error>> {
    let client = Client::new(config.url.clone(), config.bucket.clone());
    
    // Query to get distinct symbols using InfluxQL
    let query_string = format!("SHOW TAG VALUES FROM {} WITH KEY = \"symbol\"", config.bucket);
    
    let read_query = ReadQuery::new(query_string);
    
    // Execute the query and get the string result
    let query_result_string = client.query(&read_query).await?;
    
    // Parse the result as JSON
    let query_result_json: Value = serde_json::from_str(&query_result_string)?;
    
    let mut symbols = Vec::new();
    
    // Parse the JSON results
    if let Some(series_array) = query_result_json.get("series").and_then(|s| s.as_array()) {
        for series in series_array {
            if let Some(values_array) = series.get("values").and_then(|v| v.as_array()) {
                for values in values_array {
                    if let Some(values_vec) = values.as_array() {
                        if values_vec.len() >= 2 {
                            if let Value::String(symbol) = &values_vec[1] {
                                symbols.push(symbol.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    
    println!("Found {} symbols in InfluxDB", symbols.len());
    
    Ok(symbols)
}