// src/bin/check_btc_trades_detailed.rs
use std::error::Error;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, AssetConfig};
use crypto_backtest::models::{PositionType, default_strategy_config, Position};
use std::fs::File;
use std::io::Write;
use std::collections::VecDeque;

fn main() -> Result<(), Box<dyn Error>> {
    // Load candle data
    let csv_path = "data/BTC.csv";
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(csv_path)?;
    candles.retain(|c| c.volume > 0.0);
    
    // Create a log file
    let log_file = "btc_trade_check.txt";
    let mut file = File::create(log_file)?;

    // Use a longer period of data to ensure trades have time to complete
    let test_candles = candles.iter()
        .skip(candles.len().saturating_sub(5000)) // Use 5000 candles
        .cloned()
        .collect::<Vec<_>>();
    
    println!("Testing with {} candles from {} to {}", 
        test_candles.len(),
        test_candles.first().map_or("unknown", |c| &c.time),
        test_candles.last().map_or("unknown", |c| &c.time)
    );

    // Configure a simple strategy with increased threshold
    let mut config = default_strategy_config();
    config.name = "BTC Trade Check".to_string();
    config.leverage = 20.0;
    config.max_risk_per_trade = 0.01;
    config.pivot_lookback = 5;
    config.signal_lookback = 1;
    config.fib_threshold = 500.0;  // Increased from 100.0 to 500.0
    config.fib_initial = 0.5;
    config.fib_tp = 1.618;
    config.fib_sl = 0.382;
    config.fib_limit1 = 0.618;
    config.fib_limit2 = 0.786;

    // Create asset config
    let asset_config = AssetConfig {
        name: "BTC".to_string(),
        leverage: 20.0,
        spread: 0.0005,
        avg_spread: 0.001,
    };

    // Run the backtest directly using backtester
    use crypto_backtest::backtest::Backtester;
    let strategy = Strategy::new(config.clone(), asset_config.clone());
    let mut backtester = Backtester::new(10000.0, strategy);
    
    println!("Running backtest with threshold = {}...", config.fib_threshold);
    match backtester.run(&test_candles) {
        Ok(results) => {
            // Write results to log file
            writeln!(file, "BACKTEST RESULTS SUMMARY:")?;
            writeln!(file, "Total trades: {}", results.metrics.total_trades)?;
            writeln!(file, "Win rate: {:.2}%", results.metrics.win_rate * 100.0)?;
            writeln!(file, "Profit factor: {:.2}", results.metrics.profit_factor)?;
            writeln!(file, "Total profit: ${:.2}", results.metrics.total_profit)?;
            writeln!(file, "Return: {:.2}%", results.metrics.total_profit / 10000.0 * 100.0)?;
            writeln!(file, "Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0)?;
            writeln!(file, "Sharpe ratio: {:.2}", results.metrics.sharpe_ratio)?;
            
            // Print trade details - focusing on the last 5 trades
            writeln!(file, "\nDETAILS OF LAST 5 TRADES:")?;
            let last_trades = results.trades.iter().rev().take(5).collect::<Vec<_>>();
            
            for (i, trade) in last_trades.iter().enumerate() {
                writeln!(file, "Trade #{}", i + 1)?;
                writeln!(file, "  Entry Time: {}", trade.entry_time)?;
                writeln!(file, "  Exit Time: {}", trade.exit_time)?;
                writeln!(file, "  Type: {}", trade.position_type)?;
                writeln!(file, "  Entry Price: ${:.2}", trade.entry_price)?;
                writeln!(file, "  Exit Price: ${:.2}", trade.exit_price)?;
                writeln!(file, "  Size: {:.8}", trade.size)?;
                writeln!(file, "  P&L: ${:.2}", trade.pnl)?;
                writeln!(file, "  Risk %: {:.2}%", trade.risk_percent * 100.0)?;
                writeln!(file, "  Profit Factor: {:.2}", trade.profit_factor)?;
                writeln!(file, "")?;
            }
            
            // Print to console
            println!("Backtest completed successfully.");
            println!("Total trades: {}", results.metrics.total_trades);
            println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
            println!("Total profit: ${:.2}", results.metrics.total_profit);
            println!("Results saved to {}", log_file);
        },
        Err(e) => {
            println!("Error running backtest: {}", e);
            writeln!(file, "Error running backtest: {}", e)?;
        }
    }
    
    // Create a simple manual tracking system
    // Define a custom position struct
    #[derive(Debug, Clone)]
    struct TrackingPosition {
        entry_price: f64,
        stop_loss: f64,
        take_profit: f64,
        size: f64,
        limit1_price: Option<f64>,
        limit2_price: Option<f64>,
        limit1_hit: bool,
        limit2_hit: bool,
        position_type: PositionType,
        entry_time: String,
        candle_index: usize,
    }
    
    // Now let's check the implementation of position management manually
    // Create a new strategy for manual testing
    let mut strategy = Strategy::new(config.clone(), asset_config);
    let initial_balance = 10000.0;
    let mut current_balance = initial_balance;
    let mut positions: Vec<TrackingPosition> = Vec::new();
    let mut completed_trades = Vec::new();
    
    // Keep a record of the last 5 trades with all details
    let mut last_trades: VecDeque<String> = VecDeque::with_capacity(5);
    
    // Create a second log file for manual tracking
    let manual_log = "btc_manual_check.txt";
    let mut manual_file = File::create(manual_log)?;
    
    writeln!(manual_file, "MANUAL TRADE CHECKING:")?;
    
    // Process a smaller subset for detailed checking
    let check_candles = test_candles.iter().take(1000).collect::<Vec<_>>();
    
    for (i, candle) in check_candles.iter().enumerate() {
        // Check if any positions are hit
        let mut positions_to_remove = Vec::new();
        
        for (pos_idx, position) in positions.iter_mut().enumerate() {
            // Check stop loss for long positions
            if matches!(position.position_type, PositionType::Long) && candle.low <= position.stop_loss {
                // Stop loss hit for long
                let pnl = (position.stop_loss - position.entry_price) * position.size;
                current_balance += pnl;
                
                // Record trade details
                let trade_detail = format!(
                    "TRADE CLOSED:\n\
                    Type: LONG (Stop Loss)\n\
                    Entry Time: {}\n\
                    Exit Time: {}\n\
                    Duration: {} candles\n\
                    Entry Price: ${:.2}\n\
                    Exit Price: ${:.2}\n\
                    Stop Loss: ${:.2}\n\
                    Take Profit: ${:.2}\n\
                    Limit1 Price: ${:.2}\n\
                    Limit2 Price: ${:.2}\n\
                    Limit1 Hit: {}\n\
                    Limit2 Hit: {}\n\
                    P&L: ${:.2}\n\
                    -------------------------------------",
                    position.entry_time,
                    candle.time,
                    i - position.candle_index,
                    position.entry_price,
                    position.stop_loss,
                    position.stop_loss,
                    position.take_profit,
                    position.limit1_price.unwrap_or(0.0),
                    position.limit2_price.unwrap_or(0.0),
                    position.limit1_hit,
                    position.limit2_hit,
                    pnl
                );
                
                // Add to last trades queue
                last_trades.push_back(trade_detail.clone());
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE LONG #{} (Stop Loss): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    pos_idx + 1, i, position.stop_loss, pnl)?;
                completed_trades.push(format!("Long trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.stop_loss, pnl));
                positions_to_remove.push(pos_idx);
                continue;
            }
            
            // Check stop loss for short positions
            if matches!(position.position_type, PositionType::Short) && candle.high >= position.stop_loss {
                // Stop loss hit for short
                let pnl = (position.entry_price - position.stop_loss) * position.size;
                current_balance += pnl;
                
                // Record trade details
                let trade_detail = format!(
                    "TRADE CLOSED:\n\
                    Type: SHORT (Stop Loss)\n\
                    Entry Time: {}\n\
                    Exit Time: {}\n\
                    Duration: {} candles\n\
                    Entry Price: ${:.2}\n\
                    Exit Price: ${:.2}\n\
                    Stop Loss: ${:.2}\n\
                    Take Profit: ${:.2}\n\
                    Limit1 Price: ${:.2}\n\
                    Limit2 Price: ${:.2}\n\
                    Limit1 Hit: {}\n\
                    Limit2 Hit: {}\n\
                    P&L: ${:.2}\n\
                    -------------------------------------",
                    position.entry_time,
                    candle.time,
                    i - position.candle_index,
                    position.entry_price,
                    position.stop_loss,
                    position.stop_loss,
                    position.take_profit,
                    position.limit1_price.unwrap_or(0.0),
                    position.limit2_price.unwrap_or(0.0),
                    position.limit1_hit,
                    position.limit2_hit,
                    pnl
                );
                
                // Add to last trades queue
                last_trades.push_back(trade_detail.clone());
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE SHORT #{} (Stop Loss): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    pos_idx + 1, i, position.stop_loss, pnl)?;
                completed_trades.push(format!("Short trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.stop_loss, pnl));
                positions_to_remove.push(pos_idx);
                continue;
            }
            
            // Check take profit for long positions
            if matches!(position.position_type, PositionType::Long) && candle.high >= position.take_profit {
                // Take profit hit for long
                let pnl = (position.take_profit - position.entry_price) * position.size;
                current_balance += pnl;
                
                // Record trade details
                let trade_detail = format!(
                    "TRADE CLOSED:\n\
                    Type: LONG (Take Profit)\n\
                    Entry Time: {}\n\
                    Exit Time: {}\n\
                    Duration: {} candles\n\
                    Entry Price: ${:.2}\n\
                    Exit Price: ${:.2}\n\
                    Stop Loss: ${:.2}\n\
                    Take Profit: ${:.2}\n\
                    Limit1 Price: ${:.2}\n\
                    Limit2 Price: ${:.2}\n\
                    Limit1 Hit: {}\n\
                    Limit2 Hit: {}\n\
                    P&L: ${:.2}\n\
                    -------------------------------------",
                    position.entry_time,
                    candle.time,
                    i - position.candle_index,
                    position.entry_price,
                    position.take_profit,
                    position.stop_loss,
                    position.take_profit,
                    position.limit1_price.unwrap_or(0.0),
                    position.limit2_price.unwrap_or(0.0),
                    position.limit1_hit,
                    position.limit2_hit,
                    pnl
                );
                
                // Add to last trades queue
                last_trades.push_back(trade_detail.clone());
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE LONG #{} (Take Profit): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    pos_idx + 1, i, position.take_profit, pnl)?;
                completed_trades.push(format!("Long trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.take_profit, pnl));
                positions_to_remove.push(pos_idx);
                continue;
            }
            
            // Check take profit for short positions
            if matches!(position.position_type, PositionType::Short) && candle.low <= position.take_profit {
                // Take profit hit for short
                let pnl = (position.entry_price - position.take_profit) * position.size;
                current_balance += pnl;
                
                // Record trade details
                let trade_detail = format!(
                    "TRADE CLOSED:\n\
                    Type: SHORT (Take Profit)\n\
                    Entry Time: {}\n\
                    Exit Time: {}\n\
                    Duration: {} candles\n\
                    Entry Price: ${:.2}\n\
                    Exit Price: ${:.2}\n\
                    Stop Loss: ${:.2}\n\
                    Take Profit: ${:.2}\n\
                    Limit1 Price: ${:.2}\n\
                    Limit2 Price: ${:.2}\n\
                    Limit1 Hit: {}\n\
                    Limit2 Hit: {}\n\
                    P&L: ${:.2}\n\
                    -------------------------------------",
                    position.entry_time,
                    candle.time,
                    i - position.candle_index,
                    position.entry_price,
                    position.take_profit,
                    position.stop_loss,
                    position.take_profit,
                    position.limit1_price.unwrap_or(0.0),
                    position.limit2_price.unwrap_or(0.0),
                    position.limit1_hit,
                    position.limit2_hit,
                    pnl
                );
                
                // Add to last trades queue
                last_trades.push_back(trade_detail.clone());
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE SHORT #{} (Take Profit): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    pos_idx + 1, i, position.take_profit, pnl)?;
                completed_trades.push(format!("Short trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.take_profit, pnl));
                positions_to_remove.push(pos_idx);
                continue;
            }
            
            // Check limit orders (just for logging)
            if !position.limit1_hit && 
               matches!(position.position_type, PositionType::Long) && 
               candle.low <= position.limit1_price.unwrap_or(0.0) {
                position.limit1_hit = true;
                writeln!(manual_file, "LIMIT1 HIT for position #{} at ${:.2}", 
                    pos_idx + 1, position.limit1_price.unwrap_or(0.0))?;
            }
            
            if !position.limit2_hit && 
               matches!(position.position_type, PositionType::Long) && 
               candle.low <= position.limit2_price.unwrap_or(0.0) {
                position.limit2_hit = true;
                writeln!(manual_file, "LIMIT2 HIT for position #{} at ${:.2}", 
                    pos_idx + 1, position.limit2_price.unwrap_or(0.0))?;
            }
            
            if !position.limit1_hit && 
               matches!(position.position_type, PositionType::Short) && 
               candle.high >= position.limit1_price.unwrap_or(0.0) {
                position.limit1_hit = true;
                writeln!(manual_file, "LIMIT1 HIT for position #{} at ${:.2}", 
                    pos_idx + 1, position.limit1_price.unwrap_or(0.0))?;
            }
            
            if !position.limit2_hit && 
               matches!(position.position_type, PositionType::Short) && 
               candle.high >= position.limit2_price.unwrap_or(0.0) {
                position.limit2_hit = true;
                writeln!(manual_file, "LIMIT2 HIT for position #{} at ${:.2}", 
                    pos_idx + 1, position.limit2_price.unwrap_or(0.0))?;
            }
        }
        
        // Remove closed positions
        if !positions_to_remove.is_empty() {
            positions_to_remove.sort_by(|a, b| b.cmp(a)); // Sort in descending order
            for idx in positions_to_remove {
                positions.remove(idx);
            }
        }
        
        // Generate new signals and add positions
        if let Ok(signals) = strategy.analyze_candle(candle) {
            for signal in signals {
                if let Ok(position) = strategy.create_scaled_position(
                    &signal, 
                    current_balance, 
                    config.max_risk_per_trade
                ) {
                    // Record position details
                    let position_detail = format!(
                        "NEW POSITION OPENED:\n\
                        Type: {}\n\
                        Entry Time: {}\n\
                        Entry Price: ${:.2}\n\
                        Stop Loss: ${:.2} ({}% from entry)\n\
                        Take Profit: ${:.2} ({}% from entry)\n\
                        Limit1 Price: ${:.2} ({}% from entry)\n\
                        Limit2 Price: ${:.2} ({}% from entry)\n\
                        Size: {:.8}\n\
                        -------------------------------------",
                        if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" },
                        candle.time,
                        position.entry_price,
                        position.stop_loss,
                        (1.0 - position.stop_loss / position.entry_price).abs() * 100.0,
                        position.take_profit,
                        (position.take_profit / position.entry_price - 1.0).abs() * 100.0,
                        position.limit1_price.unwrap_or(0.0),
                        (1.0 - position.limit1_price.unwrap_or(0.0) / position.entry_price).abs() * 100.0,
                        position.limit2_price.unwrap_or(0.0),
                        (1.0 - position.limit2_price.unwrap_or(0.0) / position.entry_price).abs() * 100.0,
                        position.size
                    );
                    
                    writeln!(manual_file, "NEW POSITION at Candle #{}:", i)?;
                    writeln!(manual_file, "  Type: {}", if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" })?;
                    writeln!(manual_file, "  Entry: ${:.2}", position.entry_price)?;
                    writeln!(manual_file, "  Stop Loss: ${:.2}", position.stop_loss)?;
                    writeln!(manual_file, "  Take Profit: ${:.2}", position.take_profit)?;
                    writeln!(manual_file, "  Current Balance: ${:.2}", current_balance)?;
                    
                    // Convert the position to our tracking position
                    let tracking_position = TrackingPosition {
                        entry_price: position.entry_price,
                        stop_loss: position.stop_loss,
                        take_profit: position.take_profit,
                        size: position.size,
                        limit1_price: position.limit1_price,
                        limit2_price: position.limit2_price,
                        limit1_hit: false,
                        limit2_hit: false,
                        position_type: position.position_type.clone(),
                        entry_time: candle.time.clone(),
                        candle_index: i,
                    };
                    
                    positions.push(tracking_position);
                }
            }
        }
    }
    
    // Summary of manual checking
    writeln!(manual_file, "\nMANUAL CHECKING SUMMARY:")?;
    writeln!(manual_file, "Starting Balance: ${:.2}", initial_balance)?;
    writeln!(manual_file, "Final Balance: ${:.2}", current_balance)?;
    writeln!(manual_file, "Profit/Loss: ${:.2}", current_balance - initial_balance)?;
    writeln!(manual_file, "Open Positions Remaining: {}", positions.len())?;
    writeln!(manual_file, "Completed Trades: {}", completed_trades.len())?;
    
    // Write the last 5 trades with detailed information
    writeln!(manual_file, "\nDETAILS OF LAST 5 TRADES:")?;
    for (i, trade_detail) in last_trades.iter().enumerate() {
        writeln!(manual_file, "Trade #{}", last_trades.len() - i)?;
        writeln!(manual_file, "{}", trade_detail)?;
    }
    
    // Print summary to console
    println!("Manual checking completed.");
    println!("Completed Trades: {}", completed_trades.len());
    println!("Final Balance: ${:.2}", current_balance);
    println!("Manual check log saved to {}", manual_log);
    
    // Print details of open positions
    println!("\nOpen positions at end of test: {}", positions.len());
    for (i, position) in positions.iter().take(5).enumerate() {
        println!("Position #{}: {} at ${:.2}, SL=${:.2}, TP=${:.2}", 
            i + 1,
            if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" },
            position.entry_price,
            position.stop_loss,
            position.take_profit);
    }
    if positions.len() > 5 {
        println!("... and {} more open positions", positions.len() - 5);
    }
    
    // Print details of last 5 trades
    println!("\nLast 5 completed trades:");
    for (i, trade_detail) in last_trades.iter().enumerate() {
        println!("Trade #{}", last_trades.len() - i);
        println!("{}", trade_detail);
    }
    
    Ok(())
}