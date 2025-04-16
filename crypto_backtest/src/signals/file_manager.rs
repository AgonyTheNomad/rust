// src/signals/file_manager.rs
use crate::models::Signal;
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use log::*;
use serde_json::{json, to_string_pretty};

pub struct SignalFileManager {
    output_dir: String,
    version: String,
}

impl SignalFileManager {
    pub fn new(output_dir: &str, version: &str) -> Self {
        Self {
            output_dir: output_dir.to_string(),
            version: version.to_string(),
        }
    }

    pub fn write_signal(&self, signal: &Signal) -> Result<String> {
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

        // Create JSON with additional metadata
        let signal_json = json!({
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
                "strategy": "fibonacci_pivot",
                "generated_at": chrono::Utc::now().to_rfc3339()
            }
        });

        // Write to file
        let json_string = to_string_pretty(&signal_json)
            .context("Failed to serialize signal to JSON")?;
        
        let mut file = File::create(&file_path)
            .context("Failed to create signal file")?;
        
        file.write_all(json_string.as_bytes())
            .context("Failed to write signal to file")?;

        info!("Signal file written to {}", file_path.display());
        Ok(file_path.to_string_lossy().to_string())
    }

    pub fn mark_as_processed(&self, file_path: &str) -> Result<()> {
        let path = Path::new(file_path);
        
        // Read the file
        let file_content = fs::read_to_string(path)
            .context("Failed to read signal file")?;
        
        let mut signal_json: serde_json::Value = serde_json::from_str(&file_content)
            .context("Failed to parse signal JSON")?;
        
        // Update the processed flag
        signal_json["processed"] = json!(true);
        
        // Write back to file
        let json_string = to_string_pretty(&signal_json)
            .context("Failed to serialize updated signal to JSON")?;
        
        let mut file = File::create(path)
            .context("Failed to open signal file for writing")?;
        
        file.write_all(json_string.as_bytes())
            .context("Failed to write updated signal to file")?;
        
        info!("Signal file {} marked as processed", path.display());
        Ok(())
    }
}