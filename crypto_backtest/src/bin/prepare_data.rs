// src/bin/prepare_data.rs
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, Write};
use std::path::Path;
use chrono::NaiveDateTime;

fn main() -> Result<(), Box<dyn Error>> {
    // Ensure data directory exists
    let data_dir = "data";
    fs::create_dir_all(data_dir)?;
    
    println!("CSV Data Preparation Tool");
    println!("========================");
    println!("This tool will help prepare your cryptocurrency data for backtesting.");
    println!("It supports various CSV formats and ensures they're compatible with the backtesting engine.");
    
    // Get input file name from user
    println!("\nEnter the path to your CSV file:");
    let mut input_path = String::new();
    io::stdin().read_line(&mut input_path)?;
    let input_path = input_path.trim();
    
    if !Path::new(input_path).exists() {
        return Err(format!("File not found: {}", input_path).into());
    }
    
    // Detect the format
    println!("Detecting CSV format...");
    let format = detect_csv_format(input_path)?;
    println!("Detected format: {}", format);
    
    // Process the file
    println!("Processing file...");
    
    // Create output filename
    let input_filename = Path::new(input_path).file_name()
        .ok_or("Invalid input filename")?
        .to_string_lossy()
        .to_string();
    
    let output_path = format!("{}/{}", data_dir, input_filename);
    
    match format.as_str() {
        "binance" => convert_binance_format(input_path, &output_path)?,
        "yahoo" => convert_yahoo_format(input_path, &output_path)?,
        "coinbase" => convert_coinbase_format(input_path, &output_path)?,
        "kraken" => convert_kraken_format(input_path, &output_path)?,
        "generic" => {
            println!("Generic format detected. Copying to data directory...");
            fs::copy(input_path, &output_path)?;
        },
        _ => return Err("Unsupported format".into()),
    }
    
    println!("Data preparation complete!");
    println!("Processed file saved to: {}", output_path);
    println!("\nYou can now run the backtest with:");
    println!("cargo run --bin run_backtest {}", output_path);
    
    Ok(())
}

fn detect_csv_format(file_path: &str) -> Result<String, Box<dyn Error>> {
    let file = File::open(file_path)?;
    let reader = io::BufReader::new(file);
    
    // Read the header line
    if let Some(Ok(header)) = reader.lines().next() {
        // Convert to lowercase for case-insensitive matching
        let header_lower = header.to_lowercase();
        
        if header_lower.contains("timestamp") && header_lower.contains("open") && header_lower.contains("high") {
            if header_lower.contains("volume_(btc)") || header_lower.contains("volume_(base)") {
                return Ok("binance".to_string());
            } else if header_lower.contains("date") && header_lower.contains("adj close") {
                return Ok("yahoo".to_string());
            } else if header_lower.contains("time") && header_lower.contains("market") {
                return Ok("coinbase".to_string());
            } else if header_lower.contains("unixtime") || header_lower.contains("unix time") {
                return Ok("kraken".to_string());
            } else {
                return Ok("generic".to_string());
            }
        }
    }
    
    // Default to generic if we can't determine
    Ok("generic".to_string())
}

fn convert_binance_format(input_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    let input_file = File::open(input_path)?;
    let reader = io::BufReader::new(input_file);
    let mut output_file = File::create(output_path)?;
    
    // Write header
    writeln!(output_file, "Timestamp,Open,High,Low,Close,Volume")?;
    
    // Skip header line
    let mut lines = reader.lines();
    let _ = lines.next();
    
    // Process data lines
    for line in lines {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        
        if parts.len() >= 6 {
            // Binance typically has Unix timestamp in milliseconds
            let timestamp = parts[0].parse::<i64>()? / 1000;
            let time = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                .ok_or("Invalid timestamp")?
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            
            writeln!(output_file, "{},{},{},{},{},{}", 
                time, parts[1], parts[2], parts[3], parts[4], parts[5])?;
        }
    }
    
    Ok(())
}

fn convert_yahoo_format(input_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    let input_file = File::open(input_path)?;
    let reader = io::BufReader::new(input_file);
    let mut output_file = File::create(output_path)?;
    
    // Write header
    writeln!(output_file, "Timestamp,Open,High,Low,Close,Volume")?;
    
    // Skip header line
    let mut lines = reader.lines();
    let _ = lines.next();
    
    // Process data lines
    for line in lines {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        
        if parts.len() >= 6 {
            // Yahoo format typically has date in YYYY-MM-DD format
            let date = parts[0];
            
            writeln!(output_file, "{},{},{},{},{},{}", 
                date, parts[1], parts[2], parts[3], parts[4], parts[6])?;
        }
    }
    
    Ok(())
}

fn convert_coinbase_format(input_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    let input_file = File::open(input_path)?;
    let reader = io::BufReader::new(input_file);
    let mut output_file = File::create(output_path)?;
    
    // Write header
    writeln!(output_file, "Timestamp,Open,High,Low,Close,Volume")?;
    
    // Skip header line
    let mut lines = reader.lines();
    let _ = lines.next();
    
    // Process data lines
    for line in lines {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        
        if parts.len() >= 6 {
            // Coinbase typically has timestamp in ISO format
            let time = parts[0];
            
            writeln!(output_file, "{},{},{},{},{},{}", 
                time, parts[1], parts[2], parts[3], parts[4], parts[5])?;
        }
    }
    
    Ok(())
}

fn convert_kraken_format(input_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    let input_file = File::open(input_path)?;
    let reader = io::BufReader::new(input_file);
    let mut output_file = File::create(output_path)?;
    
    // Write header
    writeln!(output_file, "Timestamp,Open,High,Low,Close,Volume")?;
    
    // Skip header line
    let mut lines = reader.lines();
    let _ = lines.next();
    
    // Process data lines
    for line in lines {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        
        if parts.len() >= 6 {
            // Kraken typically has Unix timestamp
            let timestamp = parts[0].parse::<i64>()?;
            let time = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                .ok_or("Invalid timestamp")?
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            
            writeln!(output_file, "{},{},{},{},{},{}", 
                time, parts[1], parts[2], parts[3], parts[4], parts[5])?;
        }
    }
    
    Ok(())
}