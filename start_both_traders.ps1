# start_both_traders.ps1
# Script to start both Rust signal generator and Python trader

# Configuration paths
$CONFIG_DIR = ".\config"
$SIGNALS_DIR = ".\signals"
$ARCHIVE_DIR = ".\signals\archive"
$COMMANDS_DIR = ".\commands"
$LOGS_DIR = ".\logs"

# Create necessary directories
New-Item -ItemType Directory -Force -Path $CONFIG_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $SIGNALS_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $ARCHIVE_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $COMMANDS_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $LOGS_DIR | Out-Null

Write-Host "Starting Rust signal generator..."
# Start the Rust signal generator in a new window
Start-Process powershell -ArgumentList "-NoExit -Command cd rust_trader; cargo run --bin signal_generator -- --config ..\config\trader.toml --output ..\signals --archive ..\signals\archive --commands ..\commands"

Write-Host "Starting Python trader..."
# Start the Python trader in a new window
Start-Process powershell -ArgumentList "-NoExit -Command cd python; python hyperliquid_trader.py --config config.json --signals ..\signals --archive ..\signals\archive --commands ..\commands"

Write-Host "Trading system started!"
Write-Host ""
Write-Host "To test with a signal, run: python hyperliquid_test_signal.py"
Write-Host "To check status, run: .\trading_command.ps1 status"
Write-Host "To view logs, check:"
Write-Host "  - .\python\hyperliquid_trader.log for Python trader logs"
Write-Host "  - .\rust_trader\logs\signal_generator.log for Rust logs"