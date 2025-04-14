use crate::models::Candle;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;
use anyhow::{Context, Result};
use std::collections::HashMap;
use log::{debug, info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfluxDBConfig {
    pub url: String,
    pub token: String,
    pub org: String,
    pub bucket: String,
}

impl InfluxDBConfig {
    pub fn new(url: &str, token: &str, org: &str, bucket: &str) -> Self {
        Self {
            url: url.to_string(),
            token: token.to_string(),
            org: org.to_string(),
            bucket: bucket.to_string(),
        }
    }
    
    pub fn from_env() -> Result<Self> {
        // Load from environment variables or .env file
        dotenv::dotenv().ok(); // Optional loading from .env

        let url = std::env::var("INFLUXDB_URL")
            .context("INFLUXDB_URL environment variable not set")?;
        let token = std::env::var("INFLUXDB_TOKEN")
            .context("INFLUXDB_TOKEN environment variable not set")?;
        let org = std::env::var("INFLUXDB_ORG")
            .context("INFLUXDB_ORG environment variable not set")?;
        let bucket = std::env::var("INFLUXDB_BUCKET")
            .context("INFLUXDB_BUCKET environment variable not set")?;

        Ok(Self::new(&url, &token, &org, &bucket))
    }
}

#[derive(Debug, Serialize)]
struct FluxQuery {
    query: String,
}

#[derive(Debug, Clone)]
pub struct InfluxDBClient {
    config: InfluxDBConfig,
    client: Client,
}

impl InfluxDBClient {
    pub fn new(config: InfluxDBConfig) -> Result<Self> {
        // Configure HTTP client with appropriate timeouts
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { config, client })
    }

    pub async fn get_candles(
        &self,
        symbol: &str,
        start_time: &DateTime<Utc>,
        end_time: &DateTime<Utc>,
    ) -> Result<Vec<Candle>> {
        let start_rfc = start_time.to_rfc3339();
        let end_rfc = end_time.to_rfc3339();

        // Construct the Flux query
        let flux_query = format!(
            r#"
            from(bucket: "{}")
                |> range(start: {}, stop: {})
                |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "{}")
                |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")
            "#,
            self.config.bucket, start_rfc, end_rfc, symbol
        );

        debug!("Executing InfluxDB Flux query: {}", flux_query);

        // Prepare request to InfluxDB API v2
        let api_url = format!("{}/api/v2/query", self.config.url);
        
        let response = self.client
            .post(&api_url)
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/csv")
            .query(&[("org", &self.config.org)])
            .body(serde_json::to_string(&FluxQuery { query: flux_query })?)
            .send()
            .await
            .context("Failed to send InfluxDB query request")?;

        if response.status() != StatusCode::OK {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("InfluxDB query failed with status {}: {}", status, error_text));
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
            
            // Extract time field
            let time_str = record.get("_time").context("Missing _time field")?;
            let time = DateTime::parse_from_rfc3339(time_str)?.with_timezone(&Utc);
            
            // Extract price and volume data - with fallbacks for missing fields
            let open: f64 = record.get("open").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let high: f64 = record.get("high").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0); 
            let low: f64 = record.get("low").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let close: f64 = record.get("close").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let volume: f64 = record.get("volume").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let num_trades: i64 = record.get("num_trades").unwrap_or(&"0".to_string()).parse().unwrap_or(0);
            
            // Create a Candle object
            let candle = Candle {
                time: time.to_rfc3339(),
                open,
                high,
                low,
                close,
                volume,
                num_trades,
            };
            
            candles.push(candle);
        }

        // Sort by time to ensure consistent ordering
        candles.sort_by(|a, b| a.time.cmp(&b.time));
        
        info!("Retrieved {} candles from InfluxDB for {}", candles.len(), symbol);
        Ok(candles)
    }

    pub async fn get_symbols(&self) -> Result<Vec<String>> {
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
            self.config.bucket
        );

        // Prepare request to InfluxDB API v2
        let api_url = format!("{}/api/v2/query", self.config.url);
        
        let response = self.client
            .post(&api_url)
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/csv")
            .query(&[("org", &self.config.org)])
            .body(serde_json::to_string(&FluxQuery { query: flux_query })?)
            .send()
            .await
            .context("Failed to send InfluxDB symbols query")?;

        if response.status() != StatusCode::OK {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("InfluxDB symbols query failed with status {}: {}", status, error_text));
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

        info!("Retrieved {} symbols from InfluxDB", symbols.len());
        Ok(symbols)
    }

    pub async fn latest_candle(&self, symbol: &str) -> Result<Option<Candle>> {
        // Construct the Flux query to get only the latest candle for a symbol
        let flux_query = format!(
            r#"
            from(bucket: "{}")
                |> range(start: -1d)
                |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "{}")
                |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")
                |> last()
            "#,
            self.config.bucket, symbol
        );

        // Prepare request to InfluxDB API v2
        let api_url = format!("{}/api/v2/query", self.config.url);
        
        let response = self.client
            .post(&api_url)
            .header("Authorization", format!("Token {}", self.config.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/csv")
            .query(&[("org", &self.config.org)])
            .body(serde_json::to_string(&FluxQuery { query: flux_query })?)
            .send()
            .await
            .context("Failed to send InfluxDB latest candle query")?;

        if response.status() != StatusCode::OK {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("InfluxDB query failed with status {}: {}", status, error_text));
        }

        // Process CSV response
        let csv_text = response.text().await?;
        
        // Parse CSV
        let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());

        for result in rdr.deserialize() {
            let record: HashMap<String, String> = result?;
            
            // Skip header rows or other metadata
            if !record.contains_key("_time") || !record.contains_key("open") {
                continue;
            }
            
            // Extract time field
            let time_str = record.get("_time").context("Missing _time field")?;
            let time = DateTime::parse_from_rfc3339(time_str)?.with_timezone(&Utc);
            
            // Extract price and volume data
            let open: f64 = record.get("open").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let high: f64 = record.get("high").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0); 
            let low: f64 = record.get("low").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let close: f64 = record.get("close").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let volume: f64 = record.get("volume").unwrap_or(&"0".to_string()).parse().unwrap_or(0.0);
            let num_trades: i64 = record.get("num_trades").unwrap_or(&"0".to_string()).parse().unwrap_or(0);
            
            // Create a Candle object
            let candle = Candle {
                time: time.to_rfc3339(),
                open,
                high,
                low,
                close,
                volume,
                num_trades,
            };
            
            return Ok(Some(candle));
        }

        // No data found
        Ok(None)
    }

    // Method to write a signal or trade to InfluxDB for logging purposes
    pub async fn write_signal(&self, signal: &crate::models::Signal) -> Result<()> {
        let write_url = format!("{}/api/v2/write", self.config.url);
        
        let position_type = match signal.position_type {
            crate::models::PositionType::Long => "Long",
            crate::models::PositionType::Short => "Short",
        };

        // Create Line Protocol data
        let timestamp_ns = signal.timestamp.timestamp_nanos();
        let line_protocol = format!(
            "signals,symbol={},type={} price={},take_profit={},stop_loss={},strength={},reason=\"{}\" {}",
            signal.symbol, position_type, 
            signal.price, signal.take_profit, signal.stop_loss, signal.strength, signal.reason,
            timestamp_ns
        );
        
        let response = self.client
            .post(&write_url)
            .header("Authorization", format!("Token {}", self.config.token))
            .query(&[
                ("org", &self.config.org),
                ("bucket", &self.config.bucket),
                ("precision", &"ns".to_string()),
            ])
            .body(line_protocol)
            .send()
            .await
            .context("Failed to send signal data to InfluxDB")?;
            
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("InfluxDB write failed with status {}: {}", status, error_text));
        }
        
        Ok(())
    }
    
    pub async fn write_trade(&self, trade: &crate::models::Trade) -> Result<()> {
        let write_url = format!("{}/api/v2/write", self.config.url);
        
        // Create Line Protocol data
        let timestamp_ns = trade.exit_time.timestamp_nanos();
        let line_protocol = format!(
            "trades,symbol={},type={},exit_reason={} entry_price={},exit_price={},size={},pnl={},fees={} {}",
            trade.symbol, trade.position_type, 
            format!("{:?}", trade.exit_reason),
            trade.entry_price, trade.exit_price, trade.size, trade.pnl, trade.fees,
            timestamp_ns
        );
        
        let response = self.client
            .post(&write_url)
            .header("Authorization", format!("Token {}", self.config.token))
            .query(&[
                ("org", &self.config.org),
                ("bucket", &self.config.bucket),
                ("precision", &"ns".to_string()),
            ])
            .body(line_protocol)
            .send()
            .await
            .context("Failed to send trade data to InfluxDB")?;
            
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("InfluxDB write failed with status {}: {}", status, error_text));
        }
        
        Ok(())
    }
}