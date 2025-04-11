// test_strategy.rs
// Compile and run with: rustc -o test_strategy test_strategy.rs && ./test_strategy

use std::path::Path;
use std::fs;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Basic Fibonacci Strategy Test");
    println!("=============================");
    
    // First, check if we can access the data file
    let csv_path = "data/BTC.csv";
    if !Path::new(csv_path).exists() {
        eprintln!("Error: Data file not found at {}", csv_path);
        eprintln!("Please make sure you have BTC.csv in the data directory.");
        return Err("Data file not found".into());
    }
    
    // Test with a fixed set of known reasonable parameters
    let parameters = [
        // format: lookback, initial, tp, sl, threshold
        (5, 0.382, 1.0, 0.5, 10.0),
        (8, 0.5, 1.618, 0.5, 15.0),
        (10, 0.618, 2.0, 0.618, 20.0),
        (13, 0.786, 2.618, 0.382, 25.0)
    ];
    
    println!("Will test the following parameter combinations:");
    for (i, (lookback, initial, tp, sl, threshold)) in parameters.iter().enumerate() {
        println!("Set {}: Lookback={}, Initial={:.3}, TP={:.3}, SL={:.3}, Threshold={:.1}", 
                 i+1, lookback, initial, tp, sl, threshold);
    }
    
    println!("\nExecuting backtests with cargo run...");
    
    // Run a separate backtest for each parameter set
    for (i, (lookback, initial, tp, sl, threshold)) in parameters.iter().enumerate() {
        let command = format!(
            "cargo run --bin run_backtest {} {} {} {} {} {} {} {}",
            csv_path,
            "results",
            *threshold,
            *initial,
            *tp,
            *sl,
            0.5, // limit1
            1.0  // limit2
        );
        
        println!("\nRunning test set {}:\n{}", i+1, command);
        
        match std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output() {
                Ok(output) => {
                    println!("Exit status: {}", output.status);
                    
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        
                        // Extract key metrics
                        println!("Results for parameter set {}:", i+1);
                        
                        if stdout.contains("Total trades:") {
                            for line in stdout.lines() {
                                if line.contains("Total trades:") || 
                                   line.contains("Win rate:") || 
                                   line.contains("Profit factor:") || 
                                   line.contains("Total profit:") {
                                    println!("  {}", line.trim());
                                }
                            }
                        } else {
                            println!("  No trades executed with these parameters");
                        }
                    } else {
                        eprintln!("Command failed with error:");
                        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                },
                Err(e) => eprintln!("Failed to execute command: {}", e),
            }
    }
    
    println!("\nTests complete.");
    println!("If all parameter sets resulted in zero trades, there may be an issue with:");
    println!("1. The strategy logic or signal generation");
    println!("2. The candle data format or validity");
    println!("3. The default parameter ranges being too restrictive");
    
    Ok(())
}