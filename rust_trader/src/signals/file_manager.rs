// src/signals/file_manager.rs
use crate::models::{Signal, Position};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use log::*;
use serde_json::{json, to_string_pretty};
use chrono::{DateTime, Utc, Duration};

/// Manages writing signal files that can be read by the Python trader
pub struct SignalFileManager {
    /// Directory where signal files will be written
    output_dir: String,
    /// Version of the signal generator
    version: String,
}

impl SignalFileManager {
    /// Create a new signal file manager
    pub fn new(output_dir: &str, version: &str) -> Self {
        Self {
            output_dir: output_dir.to_string(),
            version: version.to_string(),
        }
    }

    /// Write a signal to a JSON file
    pub fn write_signal(&self, signal: &Signal, position: Option<&Position>, max_age_seconds: Option<i64>) -> Result<String> {
        // Check if signal is fresh enough (if max_age is provided)
        if let Some(max_age) = max_age_seconds {
            let now = Utc::now();
            let age = (now - signal.timestamp).num_seconds();
            
            if age > max_age {
                return Err(anyhow::anyhow!("Signal too old ({} seconds), not writing file", age));
            }
        }
        
        // Create directory if it doesn't exist
        fs::create_dir_all(&self.output_dir)
            .context("Failed to create signal output directory")?;
        
        // Format the filename
        let position_type_str = match signal.position_type {
            crate::models::PositionType::Long => "LONG",
            crate::models::PositionType::Short => "SHORT",
        };
        
        let timestamp = signal.timestamp.timestamp_millis();
        let filename = format!(
            "{}_{}_{}_{}.json",
            signal.symbol,
            position_type_str,
            timestamp,
            signal.id.split('-').next().unwrap_or("signal")
        );
        
        let file_path = Path::new(&self.output_dir).join(&filename);
        
        // Create JSON with additional metadata, including limit levels and TPs if available
        let mut signal_json = json!({
            "id": signal.id,
            "symbol": signal.symbol,
            "timestamp": signal.timestamp.to_rfc3339(),
            "position_type": position_type_str,
            "price": signal.price,
            "take_profit": signal.take_profit,
            "stop_loss": signal.stop_loss,
            "reason": signal.reason,
            "strength": signal.strength,
            "processed": signal.processed,
            "metadata": {
                "generator_version": self.version,
                "timestamp_ms": timestamp,
                "test": false,
                "generated_at": Utc::now().to_rfc3339()
            }
        });
        
        // Add limit and TP levels if position information is provided
        if let Some(pos) = position {
            let levels = json!({
                "limit1_price": pos.limit1_price,
                "limit2_price": pos.limit2_price,
                "limit1_size": pos.limit1_size,
                "limit2_size": pos.limit2_size,
                "new_tp1": pos.new_tp1,
                "new_tp2": pos.new_tp2,
                "position_type": format!("{:?}", pos.position_type),
                "entry_price": pos.entry_price,
                "stop_loss": pos.stop_loss,
                "take_profit": pos.take_profit,
                "position_id": pos.id,
                "size": pos.size,
                "risk_percent": pos.risk_percent,
                "status": format!("{:?}", pos.status)
            });
            
            signal_json["levels"] = levels;
        }
        
        // Write to file
        let mut file = File::create(&file_path)
            .context(format!("Failed to create signal file: {}", file_path.display()))?;
        
        let json_str = to_string_pretty(&signal_json)
            .context("Failed to serialize signal to JSON")?;
        
        file.write_all(json_str.as_bytes())
            .context("Failed to write signal to file")?;
        
        info!("Wrote signal to {}", file_path.display());
        
        Ok(filename)
    }

    /// Check for command files in the commands directory
    pub fn check_commands(&self, commands_dir: &str) -> Result<Vec<String>> {
        let dir = Path::new(commands_dir);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut commands = Vec::new();
        
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("cmd") {
                if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                    commands.push(filename.to_string());
                    
                    // Process the command file
                    if let Ok(content) = fs::read_to_string(&path) {
                        debug!("Command file content: {}", content);
                    }
                    
                    // Remove the command file after processing
                    let _ = fs::remove_file(&path);
                }
            }
        }
        
        Ok(commands)
    }

    /// Archive old signal files
    pub fn archive_old_signals(&self, archive_dir: &str, max_age_hours: i64) -> Result<usize> {
        let src_dir = Path::new(&self.output_dir);
        let dst_dir = Path::new(archive_dir);
        
        // Create archive directory if it doesn't exist
        fs::create_dir_all(dst_dir)
            .context("Failed to create archive directory")?;
            
        let cutoff_time = Utc::now() - Duration::hours(max_age_hours);
        let mut archived_count = 0;
        
        for entry in fs::read_dir(src_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Check file modification time
                let metadata = fs::metadata(&path)?;
                let modified = metadata.modified()?;
                let modified_time: DateTime<Utc> = modified.into();
                
                if modified_time < cutoff_time {
                    // Move file to archive
                    if let Some(filename) = path.file_name() {
                        let dst_path = dst_dir.join(filename);
                        fs::rename(&path, &dst_path)?;
                        archived_count += 1;
                    }
                }
            }
        }
        
        Ok(archived_count)
    }
    
    /// Check if a symbol has an active trading lock
    pub fn has_trading_lock(&self, symbol: &str) -> bool {
        let lock_path = Path::new(&self.output_dir).join(format!("{}_TRADING.lock", symbol));
        lock_path.exists()
    }

    /// Check and create a symbol lock, return true if lock was acquired
    pub fn check_and_create_lock(&self, symbol: &str) -> Result<bool> {
        let lock_path = Path::new(&self.output_dir).join(format!("{}_TRADING.lock", symbol));
        
        // Check if lock exists
        if lock_path.exists() {
            debug!("Symbol {} is locked for trading", symbol);
            
            // Check if the lock is stale (older than 10 minutes)
            let metadata = std::fs::metadata(&lock_path)?;
            let modified = metadata.modified()?;
            let modified_time: DateTime<Utc> = modified.into();
            let now = Utc::now();
            
            if now.signed_duration_since(modified_time) > Duration::minutes(10) {
                // Lock is stale, remove it
                std::fs::remove_file(&lock_path)?;
                debug!("Removed stale lock for symbol {}", symbol);
            } else {
                return Ok(false);
            }
        }
        
        // Create lock file
        let mut file = File::create(&lock_path)?;
        let now = Utc::now().to_rfc3339();
        file.write_all(now.as_bytes())?;
        
        debug!("Created trading lock for {}", symbol);
        Ok(true)
    }

    /// Release a symbol lock
    pub fn release_lock(&self, symbol: &str) -> Result<()> {
        let lock_path = Path::new(&self.output_dir).join(format!("{}_TRADING.lock", symbol));
        
        if lock_path.exists() {
            std::fs::remove_file(lock_path)?;
            debug!("Released trading lock for {}", symbol);
        }
        
        Ok(())
    }
    
    /// Create a position lock file - used to indicate a position is being managed
    pub fn create_position_lock(&self, symbol: &str, position_id: &str) -> Result<()> {
        let lock_path = Path::new(&self.output_dir).join(format!("{}_{}_POSITION.lock", symbol, position_id));
        let mut file = File::create(&lock_path)?;
        let now = Utc::now().to_rfc3339();
        file.write_all(now.as_bytes())?;
        
        debug!("Created position lock for {}/{}", symbol, position_id);
        Ok(())
    }
    
    /// Release a position lock
    pub fn release_position_lock(&self, symbol: &str, position_id: &str) -> Result<()> {
        let lock_path = Path::new(&self.output_dir).join(format!("{}_{}_POSITION.lock", symbol, position_id));
        
        if lock_path.exists() {
            std::fs::remove_file(lock_path)?;
            debug!("Released position lock for {}/{}", symbol, position_id);
        }
        
        Ok(())
    }
    
    /// Clean up stale locks
    pub fn clean_stale_locks(&self, max_age_minutes: i64) -> Result<usize> {
        let src_dir = Path::new(&self.output_dir);
        let cutoff_time = Utc::now() - Duration::minutes(max_age_minutes);
        let mut removed_count = 0;
        
        for entry in fs::read_dir(src_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename.ends_with(".lock") {
                    // Check file modification time
                    let metadata = fs::metadata(&path)?;
                    let modified = metadata.modified()?;
                    let modified_time: DateTime<Utc> = modified.into();
                    
                    if modified_time < cutoff_time {
                        // Remove stale lock
                        fs::remove_file(&path)?;
                        debug!("Removed stale lock: {}", filename);
                        removed_count += 1;
                    }
                }
            }
        }
        
        Ok(removed_count)
    }
}