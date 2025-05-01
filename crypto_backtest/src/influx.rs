// src/influx.rs
use crate::models::Candle;
use influxdb2::{Client, models::Query};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::error::Error;

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
    let client = Client::new(&config.url, &config.token, &config.org);

    // Build the time range constraint
    let time_range = match start_time {
        Some(time) => format!("start: {}", time),
        None => "start: -365d".to_string(), // Default to last year if not specified
    };

    // Construct the Flux query
    let query = Query::new(format!(
        r#"
        from(bucket: "{}")
        |> range({})
        |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "{}")
        |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")
        "#,
        config.bucket,
        time_range,
        symbol
    ));

    println!("Executing query: {}", query.get_query());

    // Execute the query
    let raw_result = client.query_raw(Some(query)).await?;
    println!("Query completed, processing results...");

    // Parse the raw results into Candles
    let mut candles = Vec::new();
    for record in raw_result {
        let values: &BTreeMap<String, influxdb2_structmap::value::Value> = &record.values;

        let time_str = values
            .get("_time")
            .and_then(|v| v.as_str())
            .ok_or("Missing _time field")?;
            
        let time_parsed = time_str.parse::<DateTime<Utc>>()?;
        
        // Format time as ISO 8601 string to match the existing Candle struct
        let time = time_parsed.to_rfc3339();

        let candle = Candle {
            time,
            open: values.get("open").and_then(|v| v.as_f64()).unwrap_or(0.0),
            high: values.get("high").and_then(|v| v.as_f64()).unwrap_or(0.0),
            low: values.get("low").and_then(|v| v.as_f64()).unwrap_or(0.0),
            close: values.get("close").and_then(|v| v.as_f64()).unwrap_or(0.0),
            volume: values.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0),
            num_trades: values.get("num_trades").and_then(|v| v.as_i64()).unwrap_or(0),
        };

        candles.push(candle);
    }

    println!("Loaded {} candles from InfluxDB", candles.len());
    
    // Sort candles by time if needed
    candles.sort_by(|a, b| a.time.cmp(&b.time));
    
    Ok(candles)
}

// Function to get all available symbols from the InfluxDB
pub async fn get_available_symbols(config: &InfluxConfig) -> Result<Vec<String>, Box<dyn Error>> {
    let client = Client::new(&config.url, &config.token, &config.org);

    // Construct a query to get distinct symbols
    let query = Query::new(format!(
        r#"
        from(bucket: "{}")
        |> range(start: -365d)
        |> filter(fn: (r) => r._measurement == "candles")
        |> group(columns: ["symbol"])
        |> distinct(column: "symbol")
        "#,
        config.bucket
    ));

    // Execute the query
    let raw_result = client.query_raw(Some(query)).await?;
    
    // Extract symbols
    let mut symbols = Vec::new();
    for record in raw_result {
        let values = &record.values;
        
        if let Some(symbol) = values.get("symbol").and_then(|v| v.as_str()) {
            symbols.push(symbol.to_string());
        }
    }
    
    println!("Found {} symbols in InfluxDB", symbols.len());
    
    Ok(symbols)
}