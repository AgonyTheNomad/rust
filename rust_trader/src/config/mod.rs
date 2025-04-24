// src/config/mod.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    // Add other config sections as needed
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub refresh_interval: u64,  // seconds
    pub data_dir: PathBuf,
    pub log_level: String,
    pub max_candles: usize,
    pub historical_days: u32,
}