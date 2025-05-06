// src/signals/file_manager.rs
use crate::models::Signal;
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
    pub fn write_signal(&self, signal: &Signal, position: Option<&Position>) -> Result<String> {
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
                "test": false
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
                "new_tp2": pos.new_tp2
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
}