// src/bin/fix_csv_format.rs
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::{Read, Write};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct InputCandle {
    // These field names match what's in your current CSV files
    time: String,
    open: f64,
    high: f64,
    low: f64, 
    close: f64,
    volume: f64,
    num_trades: i64,
}

#[derive(Debug, Serialize)]
struct OutputCandle {
    // These field names match what your backtest system expects
    timestamp: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    num_trades: i64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Get list of all CSV files in the data directory
    let data_dir = "data";
    let entries = fs::read_dir(data_dir)?;
    
    // Filter for CSV files
    let csv_files: Vec<PathBuf> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "csv" {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    
    println!("Found {} CSV files to convert", csv_files.len());
    
    // Process each file
    for (i, path) in csv_files.iter().enumerate() {
        let symbol = path.file_stem().unwrap().to_str().unwrap();
        println!("[{}/{}] Converting {} format", i+1, csv_files.len(), symbol);
        
        // Read the input file
        let mut reader = ReaderBuilder::new()
            .has_headers(true)
            .from_path(path)?;
        
        // Create a temporary output file
        let temp_path = path.with_extension("tmp");
        let mut writer = WriterBuilder::new()
            .has_headers(true)
            .from_path(&temp_path)?;
        
        // Convert each record
        let mut count = 0;
        for result in reader.deserialize() {
            let input: InputCandle = result?;
            
            // Create output record
            let output = OutputCandle {
                timestamp: input.time,
                open: input.open,
                high: input.high,
                low: input.low,
                close: input.close,
                volume: input.volume,
                num_trades: input.num_trades,
            };
            
            // Write to output file
            writer.serialize(output)?;
            count += 1;
        }
        
        // Flush and close the writer before replacing files
        writer.flush()?;
        drop(writer);
        
        // Replace the original file with the converted file
        fs::rename(&temp_path, path)?;
        
        println!("  Successfully converted {} records for {}", count, symbol);
    }
    
    println!("All files converted successfully. You can now run the backtest system.");
    Ok(())
}