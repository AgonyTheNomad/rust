#!/bin/bash
# Stop Trading System
# This script stops both the Rust signal generator and Python trader

# Configuration
COMMANDS_DIR="./commands"

# Function to check if a process is running
is_running() {
    pgrep -f "$1" > /dev/null
    return $?
}

# Create commands directory if it doesn't exist
mkdir -p "$COMMANDS_DIR"

# First try to send a graceful stop command to the signal generator
echo "Sending stop command to signal generator..."
cat > "$COMMANDS_DIR/stop_$(date +%s).cmd" << EOF
{
  "type": "stop",
  "timestamp": "$(date -Iseconds)",
  "params": {
    "reason": "User initiated shutdown"
  }
}
EOF

# Wait a moment for the command to be processed
sleep 2

# Then forcefully kill processes that are still running
if is_running "signal_generator"; then
    echo "Stopping signal generator..."
    pkill -f "signal_generator"
    sleep 1
    if is_running "signal_generator"; then
        echo "Sending SIGKILL to signal generator..."
        pkill -9 -f "signal_generator"
    fi
else
    echo "Signal generator is not running"
fi

if is_running "hyperliquid_trader.py"; then
    echo "Stopping Hyperliquid trader..."
    pkill -f "hyperliquid_trader.py"
    sleep 1
    if is_running "hyperliquid_trader.py"; then
        echo "Sending SIGKILL to Hyperliquid trader..."
        pkill -9 -f "hyperliquid_trader.py"
    fi
else
    echo "Hyperliquid trader is not running"
fi

echo ""
echo "Trading system stopped"
echo ""