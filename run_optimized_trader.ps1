# run_optimized_trader.ps1

# Configuration
$BACKTEST_DIR = ".\results"
$CONFIG_FILE = ".\config\trader.toml"
$DEFAULT_SYMBOLS = "BTC,ETH,SOL,ADA,XRP,DOT,DOGE,LINK,UNI,AVAX"
$MIN_WIN_RATE = 0.5  # 50% win rate

# Process arguments
$DRY_RUN = $false
$SYMBOLS = $DEFAULT_SYMBOLS
$BACKTEST_DIR_ARG = $BACKTEST_DIR

for ($i = 0; $i -lt $args.Length; $i++) {
    if ($args[$i] -eq "--dry-run") {
        $DRY_RUN = $true
    }
    elseif ($args[$i] -eq "--symbols" -and $i+1 -lt $args.Length) {
        $SYMBOLS = $args[$i+1]
        $i++
    }
    elseif ($args[$i] -eq "--backtest-dir" -and $i+1 -lt $args.Length) {
        $BACKTEST_DIR_ARG = $args[$i+1]
        $i++
    }
}

# Find the latest backtest results subdirectory
$LATEST_RESULTS = Get-ChildItem -Path $BACKTEST_DIR_ARG -Directory | Sort-Object LastWriteTime -Descending | Select-Object -First 1

if ($null -eq $LATEST_RESULTS) {
    Write-Host "No backtest results found in $BACKTEST_DIR_ARG"
    exit 1
}

Write-Host "Using backtest results from: $($LATEST_RESULTS.FullName)"
Write-Host "Trading symbols: $SYMBOLS"
Write-Host "Dry run mode: $DRY_RUN"

# Run the trader with optimized parameters
$DRY_RUN_ARG = ""
if ($DRY_RUN -eq $true) {
    $DRY_RUN_ARG = "--dry-run"
}

Set-Location rust_trader
cargo run --bin trader -- trade `
    --config $CONFIG_FILE `
    --symbols $SYMBOLS `
    --backtest-dir $($LATEST_RESULTS.FullName) `
    --min-win-rate $MIN_WIN_RATE `
    $DRY_RUN_ARG