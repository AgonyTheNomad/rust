use crate::models::Signal;
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use log::*;
use serde_json::{json, to_string_pretty};

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