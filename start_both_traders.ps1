# start_both_traders.ps1
# Script to start both Rust signal generator and Python trader

# Configuration paths
$CONFIG_DIR = ".\config"
$SIGNALS_DIR = ".\signals"
$ARCHIVE_DIR = ".\signals\archive"
$COMMANDS_DIR = ".\commands"
$LOGS_DIR = ".\logs"
$ACCOUNT_FILE = ".\account_info.json"

# Create necessary directories
New-Item -ItemType Directory -Force -Path $CONFIG_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $SIGNALS_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $ARCHIVE_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $COMMANDS_DIR | Out-Null
New-Item -ItemType Directory -Force -Path $LOGS_DIR | Out-Null

# Check if account file exists
if (-not (Test-Path $ACCOUNT_FILE)) {
    Write-Host "Error: Account information file not found at $ACCOUNT_FILE" -ForegroundColor Red
    Write-Host "Creating a sample account file. Please edit it with your actual balance!" -ForegroundColor Yellow
    
    # Create a sample account file
    $sampleAccount = @{
        balance = 10000.0
        available_margin = 8000.0
        used_margin = 2000.0
        timestamp = [Math]::Floor([decimal](Get-Date -UFormat %s))
        positions = @()
    } | ConvertTo-Json -Depth 4
    
    Set-Content -Path $ACCOUNT_FILE -Value $sampleAccount
    
    Write-Host "Sample account file created. Please edit $ACCOUNT_FILE before trading!" -ForegroundColor Yellow
    Write-Host "Press any key to continue or Ctrl+C to abort..." -ForegroundColor Yellow
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
}

Write-Host "Starting Rust signal generator..."
# Start the Rust signal generator in a new window with account file parameter
Start-Process powershell -ArgumentList "-NoExit -Command cd rust_trader; cargo run --bin signal_generator -- --config ..\config\trader.toml --output ..\signals --archive ..\signals\archive --commands ..\commands --account-file ..\account_info.json"

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