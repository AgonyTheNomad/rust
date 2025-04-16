#!/bin/bash
# Start Trading System
# This script starts both the Rust signal generator and Python trader

# Configuration 
SIGNALS_DIR="./signals"
ARCHIVE_DIR="./signals/archive"
COMMANDS_DIR="./commands"
CONFIG_FILE="./config/trader.toml"
PYTHON_CONFIG="./python/config.json"
LOG_DIR="./logs"

# Create necessary directories
mkdir -p "$SIGNALS_DIR" "$ARCHIVE_DIR" "$COMMANDS_DIR" "$LOG_DIR"

# Function to check if a process is running
is_running() {
    pgrep -f "$1" > /dev/null
}

# Function to start the signal generator
start_signal_generator() {
    echo "Starting signal generator..."
    # Start in background and redirect output to log file
    cd rust_trader && cargo run --bin signal_generator -- \
        --config "$CONFIG_FILE" \
        --output "$SIGNALS_DIR" \
        --archive "$ARCHIVE_DIR" \
        --commands "$COMMANDS_DIR" \
        > "../$LOG_DIR/signal_generator.log" 2>&1 &
    
    echo "Signal generator started with PID $!"
}

# Function to start the Hyperliquid trader
start_hyperliquid_trader() {
    echo "Starting Hyperliquid trader..."
    # Start in background and redirect output to log file
    cd python && python3 hyperliquid_trader.py \
        --config "$PYTHON_CONFIG" \
        --signals "../$SIGNALS_DIR" \
        --archive "../$ARCHIVE_DIR" \
        --commands "../$COMMANDS_DIR" \
        > "../$LOG_DIR/hyperliquid_trader.log" 2>&1 &
    
    echo "Hyperliquid trader started with PID $!"
}

# Check if processes are already running
if is_running "signal_generator"; then
    echo "Signal generator is already running"
else
    start_signal_generator
fi

if is_running "hyperliquid_trader.py"; then
    echo "Hyperliquid trader is already running"
else
    start_hyperliquid_trader
fi

echo ""
echo "Trading system started. Logs are available in $LOG_DIR"
echo "Use './trading_command.py status' to check system status"
echo ""

# Tail logs for debugging (optional)
if [ "$1" == "--logs" ]; then
    echo "Showing logs (Ctrl+C to exit, trading system will continue running)"
    tail -f "$LOG_DIR/signal_generator.log" "$LOG_DIR/hyperliquid_trader.log"
fi