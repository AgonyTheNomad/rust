use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::collections::VecDeque;
use crypto_backtest::fetch_data::load_candles_from_csv;
use crypto_backtest::strategy::{Strategy, AssetConfig};
use crypto_backtest::models::{PositionType, default_strategy_config};
use crypto_backtest::backtest::Backtester;

fn main() -> Result<(), Box<dyn Error>> {
    // 1) Load candle data
    let csv_path = "data/BTC.csv";
    println!("Loading data from {}...", csv_path);
    let mut candles = load_candles_from_csv(csv_path)?;
    candles.retain(|c| c.volume > 0.0);

    // 2) Prepare log file
    let log_file = "btc_trade_check_fixed.txt";
    let mut file = File::create(log_file)?;

    // 3) Slice last 5000 candles
    let test_candles = candles.iter()
        .skip(candles.len().saturating_sub(5000))
        .cloned()
        .collect::<Vec<_>>();
    println!(
        "Testing with {} candles from {} to {}",
        test_candles.len(),
        test_candles.first().map_or("unknown", |c| &c.time),
        test_candles.last().map_or("unknown", |c| &c.time)
    );

    // 4) Configure strategy
    let mut config = default_strategy_config();
    config.name = "BTC Trade Check".into();
    config.leverage = 40.0;
    config.max_risk_per_trade = 0.02;
    config.pivot_lookback = 5;
    config.signal_lookback = 1;
    config.fib_threshold = 100.0;
    config.fib_initial = 0.382;
    config.fib_tp = 1.618;
    config.fib_sl = 2.618;
    config.fib_limit1 = 0.618;
    config.fib_limit2 = 1.618;

    let asset_config = AssetConfig {
        name: "BTC".into(),
        leverage: 20.0,
        spread: 0.0005,
        avg_spread: 0.001,
    };

    // 5) Run full backtest
    let strategy = Strategy::new(config.clone(), asset_config.clone());
    let mut backtester = Backtester::new(10000.0, strategy);
    backtester.set_verbose(true);
    println!("Running backtest with threshold = {}...", config.fib_threshold);

    match backtester.run(&test_candles) {
        Ok(results) => {
            // Summary
            writeln!(file, "BACKTEST RESULTS SUMMARY:")?;
            writeln!(file, "Total trades: {}", results.metrics.total_trades)?;
            writeln!(file, "Win rate: {:.2}%", results.metrics.win_rate * 100.0)?;
            writeln!(file, "Profit factor: {:.2}", results.metrics.profit_factor)?;
            writeln!(file, "Total profit: ${:.2}", results.metrics.total_profit)?;
            writeln!(file, "Return: {:.2}%", results.metrics.total_profit / 10000.0 * 100.0)?;
            writeln!(file, "Max drawdown: {:.2}%", results.metrics.max_drawdown * 100.0)?;
            writeln!(file, "Sharpe ratio: {:.2}", results.metrics.sharpe_ratio)?;

            // DETAILS OF LAST 5 TRADES
            // DETAILS OF LAST 5 TRADES
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
                // —— NEW LINES ——
                writeln!(file, "  Stop Loss: ${:.2}",     trade.stop_loss)?;
                writeln!(file, "  Take Profit: ${:.2}",   trade.take_profit)?;
                writeln!(file, "  Limit1 Price: ${:.2}",  trade.limit1_price.unwrap_or(0.0))?;
                writeln!(file, "  Limit2 Price: ${:.2}",  trade.limit2_price.unwrap_or(0.0))?;
                writeln!(file, "  Limit1 Hit: {}",        trade.limit1_hit)?;
                writeln!(file, "  Limit2 Hit: {}",        trade.limit2_hit)?;
                if trade.limit1_hit {
                    if let Some(ts) = &trade.limit1_time {
                        writeln!(
                            file,
                            "    ↳ Limit1 was hit at {} → TP updated to ${:.2}",
                            ts,
                            trade.new_tp.unwrap_or(trade.take_profit)
                        )?;
                    }
                }
                writeln!(file, "")?;
            }


            println!("Backtest completed successfully.");
            println!("Total trades: {}", results.metrics.total_trades);
            println!("Win rate: {:.2}%", results.metrics.win_rate * 100.0);
            println!("Total profit: ${:.2}", results.metrics.total_profit);
            println!("Results saved to {}", log_file);
        }
        Err(e) => {
            println!("Error running backtest: {}", e);
            writeln!(file, "Error running backtest: {}", e)?;
        }
    }

    // 6) Manual-trade check
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

    let mut strategy = Strategy::new(config.clone(), asset_config);
    let initial_balance = 10000.0;
    let mut current_balance = initial_balance;
    let mut completed_trades = Vec::new();
    let mut last_trades: VecDeque<String> = VecDeque::with_capacity(5);
    let manual_log = "btc_manual_check_fixed.txt";
    let mut manual_file = File::create(manual_log)?;
    writeln!(manual_file, "MANUAL TRADE CHECKING (FIXED VERSION):")?;

    let check_candles = test_candles.iter().take(1000).collect::<Vec<_>>();
    let mut current_position: Option<TrackingPosition> = None;
    let mut position_count = 0;
    let mut limit1_hits = 0;
    let mut limit2_hits = 0;
    let mut tp_hits = 0;
    let mut sl_hits = 0;
    let mut winning_trades = 0;
    let mut losing_trades = 0;
    let mut should_clear_position = false;

    for (i, candle) in check_candles.iter().enumerate() {
        should_clear_position = false;
        
        // First, check if we need to close the existing position
        if let Some(position) = &mut current_position {
            // Check for stop loss for long positions
            if matches!(position.position_type, PositionType::Long) && candle.low <= position.stop_loss {
                // Stop loss hit for long
                let pnl = (position.stop_loss - position.entry_price) * position.size;
                
                // Add verification of P&L calculation for debugging
                writeln!(manual_file, "DEBUG - Long Stop Loss Hit:")?;
                writeln!(manual_file, "  Entry Price: ${:.2}", position.entry_price)?;
                writeln!(manual_file, "  Stop Loss Price: ${:.2}", position.stop_loss)?;
                writeln!(manual_file, "  Position Size: {:.8}", position.size)?;
                writeln!(manual_file, "  Calculation: (${:.2} - ${:.2}) * {:.8} = ${:.2}", 
                    position.stop_loss, position.entry_price, position.size, pnl)?;
                
                // For long positions, P&L should be negative if stop_loss < entry_price
                if pnl > 0.0 && position.stop_loss < position.entry_price {
                    writeln!(manual_file, "WARNING: Unexpected positive P&L for long position stop loss!")?;
                }
                
                current_balance += pnl;
                sl_hits += 1;
                
                if pnl > 0.0 {
                    winning_trades += 1;
                } else {
                    losing_trades += 1;
                }
                
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
                last_trades.push_back(trade_detail);
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE LONG (Stop Loss): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    i, position.stop_loss, pnl)?;
                completed_trades.push(format!("Long trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.stop_loss, pnl));
                
                // Set flag to clear position
                should_clear_position = true;
            }
            // Check for stop loss for short positions
            else if matches!(position.position_type, PositionType::Short) && candle.high >= position.stop_loss {
                // Stop loss hit for short
                let pnl = (position.entry_price - position.stop_loss) * position.size;
                
                // Add verification of P&L calculation for debugging
                writeln!(manual_file, "DEBUG - Short Stop Loss Hit:")?;
                writeln!(manual_file, "  Entry Price: ${:.2}", position.entry_price)?;
                writeln!(manual_file, "  Stop Loss Price: ${:.2}", position.stop_loss)?;
                writeln!(manual_file, "  Position Size: {:.8}", position.size)?;
                writeln!(manual_file, "  Calculation: (${:.2} - ${:.2}) * {:.8} = ${:.2}", 
                    position.entry_price, position.stop_loss, position.size, pnl)?;
                
                // For short positions, P&L should be negative if stop_loss > entry_price
                if pnl > 0.0 && position.stop_loss > position.entry_price {
                    writeln!(manual_file, "WARNING: Unexpected positive P&L for short position stop loss!")?;
                }
                
                current_balance += pnl;
                sl_hits += 1;
                
                if pnl > 0.0 {
                    winning_trades += 1;
                } else {
                    losing_trades += 1;
                }
                
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
                last_trades.push_back(trade_detail);
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE SHORT (Stop Loss): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    i, position.stop_loss, pnl)?;
                completed_trades.push(format!("Short trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.stop_loss, pnl));
                
                // Set flag to clear position
                should_clear_position = true;
            }
            // Check for take profit for long positions
            else if matches!(position.position_type, PositionType::Long) && candle.high >= position.take_profit {
                // Take profit hit for long
                let pnl = (position.take_profit - position.entry_price) * position.size;
                
                // Add verification of P&L calculation for debugging
                writeln!(manual_file, "DEBUG - Long Take Profit Hit:")?;
                writeln!(manual_file, "  Entry Price: ${:.2}", position.entry_price)?;
                writeln!(manual_file, "  Take Profit Price: ${:.2}", position.take_profit)?;
                writeln!(manual_file, "  Position Size: {:.8}", position.size)?;
                writeln!(manual_file, "  Calculation: (${:.2} - ${:.2}) * {:.8} = ${:.2}", 
                    position.take_profit, position.entry_price, position.size, pnl)?;
                
                // For long TP, P&L should be positive if take_profit > entry_price
                if pnl < 0.0 && position.take_profit > position.entry_price {
                    writeln!(manual_file, "WARNING: Unexpected negative P&L for long position take profit!")?;
                }
                
                current_balance += pnl;
                tp_hits += 1;
                winning_trades += 1;
                
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
                last_trades.push_back(trade_detail);
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE LONG (Take Profit): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    i, position.take_profit, pnl)?;
                completed_trades.push(format!("Long trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.take_profit, pnl));
                
                // Set flag to clear position
                should_clear_position = true;
            }
            // Check for take profit for short positions
            else if matches!(position.position_type, PositionType::Short) && candle.low <= position.take_profit {
                // Take profit hit for short
                let pnl = (position.entry_price - position.take_profit) * position.size;
                
                // Add verification of P&L calculation for debugging
                writeln!(manual_file, "DEBUG - Short Take Profit Hit:")?;
                writeln!(manual_file, "  Entry Price: ${:.2}", position.entry_price)?;
                writeln!(manual_file, "  Take Profit Price: ${:.2}", position.take_profit)?;
                writeln!(manual_file, "  Position Size: {:.8}", position.size)?;
                writeln!(manual_file, "  Calculation: (${:.2} - ${:.2}) * {:.8} = ${:.2}", 
                    position.entry_price, position.take_profit, position.size, pnl)?;
                
                // For short TP, P&L should be positive if entry_price > take_profit
                if pnl < 0.0 && position.entry_price > position.take_profit {
                    writeln!(manual_file, "WARNING: Unexpected negative P&L for short position take profit!")?;
                }
                
                current_balance += pnl;
                tp_hits += 1;
                winning_trades += 1;
                
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
                last_trades.push_back(trade_detail);
                if last_trades.len() > 5 {
                    last_trades.pop_front();
                }
                
                writeln!(manual_file, "CLOSE SHORT (Take Profit): Candle #{} at ${:.2}, PnL: ${:.2}", 
                    i, position.take_profit, pnl)?;
                completed_trades.push(format!("Short trade: Entry=${:.2}, Exit=${:.2}, PnL=${:.2}", 
                    position.entry_price, position.take_profit, pnl));
                
                // Set flag to clear position
                should_clear_position = true;
            }
            
            // Check limit orders (just for logging)
            if !position.limit1_hit && 
               matches!(position.position_type, PositionType::Long) && 
               candle.low <= position.limit1_price.unwrap_or(0.0) {
                position.limit1_hit = true;
                limit1_hits += 1;
                writeln!(manual_file, "LIMIT1 HIT for position #{} at ${:.2}", 
                    position_count, position.limit1_price.unwrap_or(0.0))?;
            }
            
            if !position.limit2_hit && 
               matches!(position.position_type, PositionType::Long) && 
               candle.low <= position.limit2_price.unwrap_or(0.0) {
                position.limit2_hit = true;
                limit2_hits += 1;
                writeln!(manual_file, "LIMIT2 HIT for position #{} at ${:.2}", 
                    position_count, position.limit2_price.unwrap_or(0.0))?;
            }
            
            if !position.limit1_hit && 
               matches!(position.position_type, PositionType::Short) && 
               candle.high >= position.limit1_price.unwrap_or(0.0) {
                position.limit1_hit = true;
                limit1_hits += 1;
                writeln!(manual_file, "LIMIT1 HIT for position #{} at ${:.2}", 
                    position_count, position.limit1_price.unwrap_or(0.0))?;
            }
            
            if !position.limit2_hit && 
               matches!(position.position_type, PositionType::Short) && 
               candle.high >= position.limit2_price.unwrap_or(0.0) {
                position.limit2_hit = true;
                limit2_hits += 1;
                writeln!(manual_file, "LIMIT2 HIT for position #{} at ${:.2}", 
                    position_count, position.limit2_price.unwrap_or(0.0))?;
            }
        }
        
        // Clear position if needed (outside the borrow scope)
        if should_clear_position {
            current_position = None;
        }
        
        // Only generate new signals if no position is active
        let has_open_position = current_position.is_some();
        if let Ok(signals) = strategy.analyze_candle(candle, has_open_position) {
            for signal in signals {
                if let Ok(position) = strategy.create_scaled_position(
                    &signal, 
                    current_balance, 
                    config.max_risk_per_trade
                ) {
                    // Position validation - check if price levels make sense
                    writeln!(manual_file, "\nPOSITION VALIDATION:")?;
                    writeln!(manual_file, "  Position Type: {}", 
                        if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" })?;
                    writeln!(manual_file, "  Entry Price: ${:.2}", position.entry_price)?;
                    writeln!(manual_file, "  Stop Loss: ${:.2}", position.stop_loss)?;
                    writeln!(manual_file, "  Take Profit: ${:.2}", position.take_profit)?;
                    writeln!(manual_file, "  Limit1 Price: ${:.2}", position.limit1_price.unwrap_or(0.0))?;
                    writeln!(manual_file, "  Limit2 Price: ${:.2}", position.limit2_price.unwrap_or(0.0))?;
                    writeln!(manual_file, "  Size: {:.8}", position.size)?;
                    
                    // Check the correct order of price levels
                    if matches!(position.position_type, PositionType::Long) {
                        // For long positions: stop_loss < limit2 < limit1 < entry < take_profit
                        if !(position.stop_loss < position.limit2_price.unwrap_or(0.0) && 
                            position.limit2_price.unwrap_or(0.0) < position.limit1_price.unwrap_or(0.0) && 
                            position.limit1_price.unwrap_or(0.0) < position.entry_price && 
                            position.entry_price < position.take_profit) {
                            writeln!(manual_file, "WARNING: Invalid price levels for long position!")?;
                        }
                    } else {
                        // For short positions: stop_loss > limit2 > limit1 > entry > take_profit
                        if !(position.stop_loss > position.limit2_price.unwrap_or(0.0) && 
                            position.limit2_price.unwrap_or(0.0) > position.limit1_price.unwrap_or(0.0) && 
                            position.limit1_price.unwrap_or(0.0) > position.entry_price && 
                            position.entry_price > position.take_profit) {
                            writeln!(manual_file, "WARNING: Invalid price levels for short position!")?;
                        }
                    }
                    
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
                    
                    // Use position_detail to avoid warning
                    writeln!(manual_file, "{}", position_detail)?;
                    
                    position_count += 1;
                    writeln!(manual_file, "NEW POSITION #{} at Candle #{}:", position_count, i)?;
                    writeln!(manual_file, "  Type: {}", 
                        if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" })?;
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
                    
                    // Set as current position - only take one position at a time
                    current_position = Some(tracking_position);
                    
                    // Break after creating a position - we only want one position at a time
                    break;
                }
            }
        }
    }
    
    // Summary of manual checking
    writeln!(manual_file, "\nMANUAL CHECKING SUMMARY:")?;
    writeln!(manual_file, "Starting Balance: ${:.2}", initial_balance)?;
    writeln!(manual_file, "Final Balance: ${:.2}", current_balance)?;
    writeln!(manual_file, "Profit/Loss: ${:.2}", current_balance - initial_balance)?;
    writeln!(manual_file, "Open Positions Remaining: {}", if current_position.is_some() { 1 } else { 0 })?;
    writeln!(manual_file, "Completed Trades: {}", completed_trades.len())?;
    writeln!(manual_file, "Win Rate: {:.2}%", 
        if winning_trades + losing_trades > 0 {
            (winning_trades as f64 / (winning_trades + losing_trades) as f64) * 100.0
        } else { 
            0.0 
        })?;
    
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
    
    // Print details of open position if any
    if let Some(position) = &current_position {
        println!("\nOpen position at end of test:");
        println!("Type: {} at ${:.2}, SL=${:.2}, TP=${:.2}", 
            if matches!(position.position_type, PositionType::Long) { "LONG" } else { "SHORT" },
            position.entry_price,
            position.stop_loss,
            position.take_profit);
    } else {
        println!("\nNo open positions at end of test");
    }
    
    // Print details of last 5 trades
    println!("\nLast 5 completed trades:");
    for (i, trade_detail) in last_trades.iter().enumerate() {
        println!("Trade #{}", last_trades.len() - i);
        println!("{}", trade_detail);
    }
    
    Ok(())
}