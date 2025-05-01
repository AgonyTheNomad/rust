// src/bin/run_backtest.rs
use crypto_backtest::backtest::Backtester;
use crypto_backtest::strategy::{Strategy, StrategyConfig, AssetConfig};
use crypto_backtest::models::{default_strategy_config, default_asset_config};
use tokio;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set up logging
    env_logger::init();
    println!("Starting cryptocurrency backtesting system...");
    
    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    
    // Parse symbol from command line or use default
    let symbol = args.get(1)
        .cloned()
        .unwrap_or_else(|| "BTC".to_string());
    
    let output_dir = args.get(2)
        .cloned()
        .unwrap_or_else(|| "results".to_string());
    
    // Ensure the output directory exists
    std::fs::create_dir_all(&output_dir)?;
    
    // Load candles - using local CSV file
    println!("Loading candle data for {} from CSV...", symbol);
    let candle_path = format!("data/{}.csv", symbol);
    let mut candles = crypto_backtest::fetch_data::load_candles_from_csv(&candle_path)?;
    
    // Apply data quality filters
    candles.retain(|c| c.volume > 0.0);
    println!("Loaded {} valid candles", candles.len());
    
    if candles.is_empty() {
        return Err("No candle data loaded".into());
    }
    
    // Print the date range
    let start_date = &candles.first().unwrap().time;
    let end_date = &candles.last().unwrap().time;
    println!("Date range: {} to {}", start_date, end_date);
    
    // Define strategy configuration
    let mut config = default_strategy_config();
    config.name = format!("{} Backtest", symbol);
    config.leverage = 20.0;
    config.max_risk_per_trade = 0.01;
    config.pivot_lookback = 5;
    config.signal_lookback = 1;
    config.fib_threshold = 10.0;
    config.fib_initial = 0.382;
    config.fib_tp = 0.618;
    config.fib_sl = 0.236;
    config.fib_limit1 = 0.5;
    config.fib_limit2 = 0.618;
    
    // Create asset configuration
    let asset_config = default_asset_config(&symbol);
    
    // Create strategy and backtester
    let strategy = Strategy::new(config.clone(), asset_config);
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
        let trade_file = format!("{}/trades_{}.csv", output_dir, symbol);
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
    
    // Save performance metrics to JSON
    let metrics_file = format!("{}/metrics_{}.json", output_dir, symbol);
    println!("Saving performance metrics to {}...", metrics_file);
    let metrics_json = serde_json::json!({
        "strategy_config": {
            "name": config.name,
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
            "data_source": "CSV",
        }
    });
    
    let mut metrics_file = File::create(metrics_file)?;
    metrics_file.write_all(serde_json::to_string_pretty(&metrics_json)?.as_bytes())?;
    
    println!("\nBacktest completed successfully. Results saved to {} directory.", output_dir);
    
    Ok(())
}