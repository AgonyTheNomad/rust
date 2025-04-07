use std::error::Error;
use std::fs::File;
use std::path::Path;
use csv::ReaderBuilder;
use serde::Deserialize;
use crate::models::Candle;

/// Custom struct to match your CSV format
#[derive(Debug, Deserialize)]
struct CsvCandle {
    Timestamp: String,
    Open: f64,
    High: f64,
    Low: f64,
    Close: f64,
    Volume: f64,
}

/// Load candle data from a CSV file
pub fn load_candles_from_csv(file_path: &str) -> Result<Vec<Candle>, Box<dyn Error>> {
    println!("Loading candle data from {}", file_path);
    
    // Check if file exists
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path).into());
    }
    
    let file = File::open(path)?;
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file);
    
    let mut candles = Vec::new();
    for result in rdr.deserialize() {
        let csv_candle: CsvCandle = result?;
        
        // Format timestamp to ISO 8601 format if necessary
        let time = if csv_candle.Timestamp.contains('T') {
            csv_candle.Timestamp
        } else {
            // Convert from "2024-08-06 08:00:00" to "2024-08-06T08:00:00Z"
            csv_candle.Timestamp.replace(' ', "T") + "Z"
        };
        
        // Convert to your Candle model
        let candle = Candle {
            time,
            open: csv_candle.Open,
            high: csv_candle.High,
            low: csv_candle.Low,
            close: csv_candle.Close,
            volume: csv_candle.Volume,
            num_trades: 0, // This field isn't in your CSV, so default to 0
        };
        
        candles.push(candle);
    }
    
    println!("Loaded {} candles from CSV", candles.len());
    
    // Sort candles by time if needed
    // candles.sort_by(|a, b| a.time.cmp(&b.time));
    
    Ok(candles)
}

/// Save candles to CSV file
pub fn save_candles_to_csv(candles: &[Candle], file_path: &str) -> Result<(), Box<dyn Error>> {
    println!("Saving {} candles to {}", candles.len(), file_path);
    
    let path = Path::new(file_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let mut wtr = csv::Writer::from_path(path)?;
    
    for candle in candles {
        wtr.serialize(candle)?;
    }
    
    wtr.flush()?;
    println!("Successfully saved candles to CSV");
    
    Ok(())
}