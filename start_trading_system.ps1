# start_trading_system.ps1

# Configuration 
$SIGNALS_DIR = ".\signals"
$ARCHIVE_DIR = ".\signals\archive"
$COMMANDS_DIR = ".\commands"
$CONFIG_FILE = ".\config\trader.toml"
$PYTHON_CONFIG = ".\python\config.json"
$LOG_DIR = ".\logs"

# Create necessary directories
New-Item -ItemType Directory -Force -Path $SIGNALS_DIR
New-Item -ItemType Directory -Force -Path $ARCHIVE_DIR
New-Item -ItemType Directory -Force -Path $COMMANDS_DIR
New-Item -ItemType Directory -Force -Path $LOG_DIR

# Function to check if a process is running
function Is-ProcessRunning {
    param(
        [string]$ProcessName
    )
    
    return Get-Process | Where-Object { $_.ProcessName -match $ProcessName }
}

# Function to start the signal generator
function Start-SignalGenerator {
    Write-Host "Starting signal generator..."
    $logFile = Join-Path -Path $LOG_DIR -ChildPath "signal_generator.log"
    
    Push-Location rust_trader
    Start-Process -FilePath "cmd.exe" -ArgumentList "/c cargo run --bin signal_generator -- --config $CONFIG_FILE --output $SIGNALS_DIR --archive $ARCHIVE_DIR --commands $COMMANDS_DIR > $logFile 2>&1"
    Pop-Location
    
    Write-Host "Signal generator started"
}

# Function to start the Hyperliquid trader
function Start-HyperliquidTrader {
    Write-Host "Starting Hyperliquid trader..."
    $logFile = Join-Path -Path $LOG_DIR -ChildPath "hyperliquid_trader.log"
    
    Push-Location python
    Start-Process -FilePath "python" -ArgumentList "hyperliquid_trader.py --config $PYTHON_CONFIG --signals $SIGNALS_DIR --archive $ARCHIVE_DIR --commands $COMMANDS_DIR > $logFile 2>&1"
    Pop-Location
    
    Write-Host "Hyperliquid trader started"
}

# Check if processes are already running
if (Is-ProcessRunning "signal_generator") {
    Write-Host "Signal generator is already running"
}
else {
    Start-SignalGenerator
}

if (Is-ProcessRunning "python" -and (Get-Process python).CommandLine -contains "hyperliquid_trader.py") {
    Write-Host "Hyperliquid trader is already running"
}
else {
    Start-HyperliquidTrader
}

Write-Host ""
Write-Host "Trading system started. Logs are available in $LOG_DIR"
Write-Host "Use '.\trading_command.ps1 status' to check system status"
Write-Host ""

# View logs (optional)
if ($args -contains "--logs") {
    Write-Host "Showing logs (Ctrl+C to exit, trading system will continue running)"
    Get-Content -Path "$LOG_DIR\signal_generator.log", "$LOG_DIR\hyperliquid_trader.log" -Wait
}