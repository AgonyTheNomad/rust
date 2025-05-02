// src/bin/backtest_all.rs
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    // Get list of all CSV files in the data directory
    let data_dir = "data";
    let entries = fs::read_dir(data_dir)?;
    
    // Filter for CSV files
    let csv_files: Vec<_> = entries
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
    
    println!("Found {} CSV files to backtest", csv_files.len());
    
    // Create results directory
    let results_dir = "results";
    fs::create_dir_all(results_dir)?;
    
    // Run backtest for each file
    for (i, path) in csv_files.iter().enumerate() {
        let symbol = path.file_stem().unwrap().to_str().unwrap();
        println!("[{}/{}] Running backtest for {}", i+1, csv_files.len(), symbol);
        
        // Run the backtest using the run_backtest binary
        let status = Command::new("cargo")
            .args(&["run", "--bin", "run_backtest", symbol, results_dir])
            .status()?;
        
        if status.success() {
            println!("  Successfully completed backtest for {}", symbol);
        } else {
            println!("  Error running backtest for {}", symbol);
        }
        
        println!("------------------------------------------------");
    }
    
    println!("All backtests completed. Results saved to {}", results_dir);
    Ok(())
}