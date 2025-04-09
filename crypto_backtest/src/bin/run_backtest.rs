// src/bin/run_backtest.rs
use crypto_backtest::backtest::Backtester;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, StrategyConfig};
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::env;

fn main() -> Result<(), Box<dyn Error>> {
    // Set up logging
    env_logger::init();
    println!("Starting cryptocurrency backtesting system...");
    
    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    
    // Parse command line arguments or use defaults
    let csv_path = args.get(1)
        .cloned()
        .unwrap_or_else(|| "data/BTC.csv".to_string());
    
    let output_dir = args.get(2)
        .cloned()
        .unwrap_or_else(|| "results".to_string());
    
    // Ensure the output directory exists
    std::fs::create_dir_all(&output_dir)?;
    
    // Load the market data
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(&csv_path)?;
    
    // Apply data quality filters
    candles.retain(|c| c.volume > 0.0);
    println!("Loaded {} candles", candles.len());
    
    if candles.is_empty() {
        return Err("No candle data loaded".into());
    }
    
    // Print the date range
    let start_date = &candles.first().unwrap().time;
    let end_date = &candles.last().unwrap().time;
    println!("Date range: {} to {}", start_date, end_date);
    
    // Define strategy configuration
    let config = StrategyConfig {
        initial_balance: 10_000.0,
        leverage: 10.0,
        max_risk_per_trade: 0.01,
        pivot_lookback: 5,
        signal_lookback: 1,
        fib_threshold: 10.0,
        fib_initial: 0.382,  // Fibonacci level for entry
        fib_tp: 0.618,       // Take profit level
        fib_sl: 0.236,       // Stop loss level
        fib_limit1: 0.5,     // First scaling level
        fib_limit2: 0.618,   // Second scaling level
    };
    
    // Create strategy and backtester
    let strategy = Strategy::new(config.clone());
    let mut backtester = Backtester::new(config.initial_balance, strategy);
    
    // Run the backtest
    println!("Running backtest...");
    let start_time = std::time::Instant::now();
    let results = backtester.run(&candles)?;
    let elapsed = start_time.elapsed();
    
    // Calculate overall performance
    let final_balance = config.initial_balance + results.metrics.total_profit;
    let total_return_pct = (results.metrics.total_profit / config.initial_balance) * 100.0;
    
    // Print the summary
    println!("\nBacktest Summary:");
    println!("=================");
    println!("Execution time: {:.2?}", elapsed);
    println!("Initial balance: ${:.2}", config.initial_balance);
    println!("Final balance: ${:.2}", final_balance);
    println!("Total trades: {}", results.metrics.total_trades);
    println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
    println!("Profit factor: {:.2}", results.metrics.profit_factor);
    println!("Total profit: ${:.2}", results.metrics.total_profit);
    println!("Return: {:.2}%", total_return_pct);
    println!("Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0);
    println!("Sharpe ratio: {:.2}", results.metrics.sharpe_ratio);
    println!("Sortino ratio: {:.2}", results.metrics.sortino_ratio);
    println!("Risk/reward ratio: {:.2}", results.metrics.risk_reward_ratio);
    
    // Save trade list to CSV
    if !results.trades.is_empty() {
        let trade_file = format!("{}/trades.csv", output_dir);
        println!("\nSaving trade list to {}...", trade_file);
        let mut writer = csv::Writer::from_path(trade_file)?;
        
        writer.write_record(&[
            "Entry Time", "Exit Time", "Type", "Entry Price", "Exit Price", 
            "Size", "P&L", "Risk %", "Profit Factor", "Margin Used"
        ])?;
        
        for trade in &results.trades {
            writer.write_record(&[
                trade.entry_time.clone(),
                trade.exit_time.clone(),
                trade.position_type.clone(),
                format!("{:.2}", trade.entry_price),
                format!("{:.2}", trade.exit_price),
                format!("{:.6}", trade.size),
                format!("{:.2}", trade.pnl),
                format!("{:.2}%", trade.risk_percent * 100.0),
                format!("{:.4}", trade.profit_factor),
                format!("{:.2}", trade.margin_used),
            ])?;
        }
        
        writer.flush()?;
    }
    
    // Save performance metrics
    let metrics_file = format!("{}/metrics.json", output_dir);
    println!("Saving performance metrics to {}...", metrics_file);
    let metrics_json = serde_json::json!({
        "strategy_config": {
            "initial_balance": config.initial_balance,
            "leverage": config.leverage,
            "max_risk_per_trade": config.max_risk_per_trade,
            "pivot_lookback": config.pivot_lookback,
            "signal_lookback": config.signal_lookback,
            "fib_threshold": config.fib_threshold,
            "fib_initial": config.fib_initial,
            "fib_tp": config.fib_tp,
            "fib_sl": config.fib_sl,
        },
        "performance": {
            "total_trades": results.metrics.total_trades,
            "winning_trades": (results.metrics.win_rate * results.metrics.total_trades as f64).round() as usize,
            "losing_trades": results.metrics.total_trades - (results.metrics.win_rate * results.metrics.total_trades as f64).round() as usize,
            "win_rate": results.metrics.win_rate,
            "profit_factor": results.metrics.profit_factor,
            "total_profit": results.metrics.total_profit,
            "total_return_percent": total_return_pct,
            "max_drawdown": results.metrics.max_drawdown,
            "sharpe_ratio": results.metrics.sharpe_ratio,
            "sortino_ratio": results.metrics.sortino_ratio,
            "risk_reward_ratio": results.metrics.risk_reward_ratio,
            "final_balance": final_balance,
        },
        "execution_info": {
            "start_date": start_date,
            "end_date": end_date,
            "candle_count": candles.len(),
            "execution_time_ms": elapsed.as_millis(),
        }
    });
    
    let mut metrics_file = File::create(metrics_file)?;
    metrics_file.write_all(serde_json::to_string_pretty(&metrics_json)?.as_bytes())?;
    
    // Save equity curve to a text file
    let equity_file = format!("{}/equity_curve.csv", output_dir);
    println!("Saving equity curve to {}...", equity_file);
    let mut equity_writer = csv::Writer::from_path(equity_file)?;
    
    // Add headers
    equity_writer.write_record(&["Date", "Equity"])?;
    
    // If we have trades, we can build a daily equity curve
    if !results.trades.is_empty() {
        // Starting with initial balance
        let mut equity = config.initial_balance;
        
        // For every trade, update the equity and write it out
        for trade in &results.trades {
            equity += trade.pnl;
            equity_writer.write_record(&[
                trade.exit_time.clone(),
                format!("{:.2}", equity),
            ])?;
        }
    }
    
    equity_writer.flush()?;
    
    // Generate a text report
    let report_file = format!("{}/report.txt", output_dir);
    println!("Saving detailed report to {}...", report_file);
    let mut report = File::create(report_file)?;
    
    // Write the report
    write!(report, "Cryptocurrency Backtesting Report\n")?;
    write!(report, "===============================\n\n")?;
    write!(report, "Strategy: Fibonacci Pivot Points\n")?;
    write!(report, "Date Range: {} to {}\n", start_date, end_date)?;
    write!(report, "Instrument: {}\n\n", Path::new(&csv_path).file_stem().unwrap().to_string_lossy())?;
    
    write!(report, "Strategy Configuration:\n")?;
    write!(report, "  Initial Balance: ${:.2}\n", config.initial_balance)?;
    write!(report, "  Leverage: {}x\n", config.leverage)?;
    write!(report, "  Max Risk Per Trade: {:.2}%\n", config.max_risk_per_trade * 100.0)?;
    write!(report, "  Pivot Lookback: {} candles\n", config.pivot_lookback)?;
    write!(report, "  Signal Lookback: {} candles\n", config.signal_lookback)?;
    write!(report, "  Fibonacci Entry Level: {:.3}\n", config.fib_initial)?;
    write!(report, "  Fibonacci Take Profit: {:.3}\n", config.fib_tp)?;
    write!(report, "  Fibonacci Stop Loss: {:.3}\n\n", config.fib_sl)?;
    
    write!(report, "Performance Metrics:\n")?;
    write!(report, "  Total Trades: {}\n", results.metrics.total_trades)?;
    write!(report, "  Win Rate: {:.2}%\n", results.metrics.win_rate * 100.0)?;
    write!(report, "  Profit Factor: {:.2}\n", results.metrics.profit_factor)?;
    write!(report, "  Final Balance: ${:.2}\n", final_balance)?;
    write!(report, "  Total Profit: ${:.2}\n", results.metrics.total_profit)?;
    write!(report, "  Total Return: {:.2}%\n", total_return_pct)?;
    write!(report, "  Max Drawdown: {:.2}%\n", results.metrics.max_drawdown * 100.0)?;
    write!(report, "  Sharpe Ratio: {:.2}\n", results.metrics.sharpe_ratio)?;
    write!(report, "  Sortino Ratio: {:.2}\n", results.metrics.sortino_ratio)?;
    write!(report, "  Risk/Reward Ratio: {:.2}\n\n", results.metrics.risk_reward_ratio)?;
    
    // Add trade statistics
    if !results.trades.is_empty() {
        let win_trades = results.trades.iter().filter(|t| t.pnl > 0.0).count();
        let loss_trades = results.trades.iter().filter(|t| t.pnl < 0.0).count();
        
        let avg_win = results.trades.iter()
            .filter(|t| t.pnl > 0.0)
            .map(|t| t.pnl)
            .sum::<f64>() / win_trades.max(1) as f64;
            
        let avg_loss = results.trades.iter()
            .filter(|t| t.pnl < 0.0)
            .map(|t| t.pnl.abs())
            .sum::<f64>() / loss_trades.max(1) as f64;
            
        let largest_win = results.trades.iter()
            .filter(|t| t.pnl > 0.0)
            .map(|t| t.pnl)
            .fold(0.0, f64::max);
            
        let largest_loss = results.trades.iter()
            .filter(|t| t.pnl < 0.0)
            .map(|t| t.pnl.abs())
            .fold(0.0, f64::max);
        
        write!(report, "Trade Statistics:\n")?;
        write!(report, "  Winning Trades: {}\n", win_trades)?;
        write!(report, "  Losing Trades: {}\n", loss_trades)?;
        write!(report, "  Average Win: ${:.2}\n", avg_win)?;
        write!(report, "  Average Loss: ${:.2}\n", avg_loss)?;
        write!(report, "  Largest Win: ${:.2}\n", largest_win)?;
        write!(report, "  Largest Loss: ${:.2}\n", largest_loss)?;
        
        // Long vs Short performance
        let long_trades = results.trades.iter()
            .filter(|t| t.position_type == "Long")
            .count();
            
        let short_trades = results.trades.iter()
            .filter(|t| t.position_type == "Short")
            .count();
            
        let long_pnl = results.trades.iter()
            .filter(|t| t.position_type == "Long")
            .map(|t| t.pnl)
            .sum::<f64>();
            
        let short_pnl = results.trades.iter()
            .filter(|t| t.position_type == "Short")
            .map(|t| t.pnl)
            .sum::<f64>();
            
        write!(report, "\nDirection Analysis:\n")?;
        write!(report, "  Long Trades: {} ({:.2}%)\n", 
            long_trades, 
            (long_trades as f64 / results.metrics.total_trades as f64) * 100.0)?;
        write!(report, "  Long P&L: ${:.2}\n", long_pnl)?;
        write!(report, "  Short Trades: {} ({:.2}%)\n", 
            short_trades, 
            (short_trades as f64 / results.metrics.total_trades as f64) * 100.0)?;
        write!(report, "  Short P&L: ${:.2}\n", short_pnl)?;
    }
    
    println!("\nBacktest completed successfully. Results saved to {} directory.", output_dir);
    
    Ok(())
}