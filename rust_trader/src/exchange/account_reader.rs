// src/exchange/account_reader.rs
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use log::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub size: f64,
    pub entry_price: f64,
    pub side: String,
    pub unrealized_pnl: f64,
    pub mark_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balance: f64,
    pub available_margin: f64,
    pub used_margin: f64,
    pub timestamp: f64,
    pub positions: Vec<Position>,
}

pub struct AccountReader {
    pub account_file_path: String,
    pub max_age_seconds: u64,
}

impl AccountReader {
    pub fn new(account_file_path: &str, max_age_seconds: u64) -> Self {
        Self {
            account_file_path: account_file_path.to_string(),
            max_age_seconds,
        }
    }
    
    /// Read account information from the file written by Python
    pub fn read_account_info(&self) -> Result<AccountInfo> {
        let path = Path::new(&self.account_file_path);
        
        // Check if file exists
        if !path.exists() {
            return Err(anyhow::anyhow!("Account info file not found: {}", self.account_file_path));
        }
        
        // Read file
        let mut file = File::open(path).context("Failed to open account info file")?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).context("Failed to read account info file")?;
        
        // Parse JSON
        let account_info: AccountInfo = serde_json::from_str(&contents)
            .context("Failed to parse account info JSON")?;
        
        // Check if file is too old
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();
            
        let age_seconds = now - account_info.timestamp;
        
        if age_seconds > self.max_age_seconds as f64 {
            warn!("Account info file is {} seconds old (max age: {})",
                age_seconds, self.max_age_seconds);
        }
        
        debug!("Read account info: balance=${:.2}, {} positions",
            account_info.balance, account_info.positions.len());
        
        Ok(account_info)
    }
    
    /// Get current balance
    pub fn get_balance(&self) -> Result<f64> {
        match self.read_account_info() {
            Ok(info) => Ok(info.balance),
            Err(e) => {
                // Instead of returning a default value, propagate the error
                Err(anyhow::anyhow!("Failed to read balance from account info file: {}", e))
            }
        }
    }
    
    /// Check if a symbol has an open position
    pub fn has_open_position(&self, symbol: &str) -> Result<bool> {
        match self.read_account_info() {
            Ok(info) => {
                let has_position = info.positions.iter()
                    .any(|p| p.symbol == symbol);
                Ok(has_position)
            },
            Err(e) => {
                warn!("Failed to check positions from account info file: {}", e);
                // Default to false if we can't read the file
                Ok(false)
            }
        }
    }
}