# run_influx_backtest.ps1 - A script to run backtests with InfluxDB data for Windows

# Default configuration
$Symbol = "BTC"
$OutputDir = "results"

# Check for parameters
$All = $false
$ShowHelp = $false

# Parse simple command line arguments
for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        "-Symbol" {
            $Symbol = $args[$i+1]
            $i++
        }
        "-OutputDir" {
            $OutputDir = $args[$i+1]
            $i++
        }
        "-List" {
            Write-Host "Listing available symbols..."
            cargo run --bin influx_utils symbols
            exit
        }
        "-Info" {
            $infoSymbol = $args[$i+1]
            Write-Host "Getting information for $infoSymbol..."
            cargo run --bin influx_utils info $infoSymbol
            exit
        }
        "-Export" {
            $exportSymbol = $args[$i+1]
            $csvFile = "${exportSymbol}_candles.csv"
            if ($args.Count -gt $i+2 -and -not $args[$i+2].StartsWith("-")) {
                $csvFile = $args[$i+2]
                $i++
            }
            Write-Host "Exporting candles for $exportSymbol to $csvFile..."
            cargo run --bin influx_utils export $exportSymbol $csvFile
            exit
        }
        "-All" {
            $All = $true
        }
        "-Optimize" {
            $optimizeSymbol = $args[$i+1]
            Write-Host "Running parameter optimization for $optimizeSymbol..."
            cargo run --bin optimize_influx $optimizeSymbol
            exit
        }
        "-Help" {
            $ShowHelp = $true
        }
    }
}

# Function to show help information
function Show-Help {
    Write-Host "Cryptocurrency Backtest System with InfluxDB data"
    Write-Host "================================================"
    Write-Host "Usage: ./run_influx_backtest.ps1 [options]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -Symbol <SYMBOL>       Symbol to backtest (default: BTC)"
    Write-Host "  -OutputDir <DIR>       Output directory (default: results)"
    Write-Host "  -List                  List available symbols"
    Write-Host "  -Info <SYMBOL>         Show information about a symbol"
    Write-Host "  -Export <SYMBOL> [FILE] Export candles for a symbol to CSV"
    Write-Host "  -All                   Run backtest for all available symbols"
    Write-Host "  -Optimize <SYMBOL>     Run parameter optimization for a symbol"
    Write-Host "  -Help                  Show this help message"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  ./run_influx_backtest.ps1 -Symbol ETH -OutputDir results/ethereum"
    Write-Host "  ./run_influx_backtest.ps1 -List"
    Write-Host "  ./run_influx_backtest.ps1 -Info BTC"
    Write-Host "  ./run_influx_backtest.ps1 -Export ETH eth_candles.csv"
    Write-Host "  ./run_influx_backtest.ps1 -Optimize BTC"
    Write-Host "  ./run_influx_backtest.ps1 -All"
    Write-Host ""
}

# Show help if requested
if ($ShowHelp) {
    Show-Help
    exit
}

# Create output directory if it doesn't exist
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
    Write-Host "Created output directory: $OutputDir"
}

# Run backtest for all available symbols or a single symbol
if ($All) {
    Write-Host "Running backtest for all available symbols..."
    
    # Run the utility to get symbols and extract just the symbol names
    $symbolsOutput = cargo run --bin influx_utils symbols
    
    # Process the output to extract just the symbol names
    $symbols = @()
    foreach ($line in $symbolsOutput) {
        if ($line -match '^\s*\d+:\s+(.+)$') {
            $symbols += $matches[1]
        }
    }
    
    Write-Host "Found $($symbols.Count) symbols to process"
    
    # Create a summary CSV file
    $summaryFile = Join-Path $OutputDir "summary_report.csv"
    "Symbol,Total Trades,Win Rate,Profit,Return %,Max Drawdown,Sharpe Ratio" | Out-File $summaryFile -Encoding UTF8
    
    # Process each symbol
    $current = 1
    foreach ($sym in $symbols) {
        Write-Host "[$current/$($symbols.Count)] Processing symbol: $sym"
        
        # Create symbol-specific output directory
        $symbolDir = Join-Path $OutputDir $sym
        if (-not (Test-Path $symbolDir)) {
            New-Item -ItemType Directory -Path $symbolDir | Out-Null
        }
        
        # Run the backtest for this symbol
        Write-Host "Running backtest for $sym. Output will be saved to $symbolDir"
        cargo run --bin run_influx_backtest $sym $symbolDir
        
        # Extract metrics for summary
        $metricsFile = Join-Path $symbolDir "metrics_$sym.json"
        if (Test-Path $metricsFile) {
            try {
                $metrics = Get-Content $metricsFile | ConvertFrom-Json
                
                # Extract key metrics
                $totalTrades = $metrics.performance.total_trades
                $winRate = $metrics.performance.win_rate
                $profit = $metrics.performance.total_profit
                $returnPct = $metrics.performance.total_return_percent
                $maxDrawdown = $metrics.performance.max_drawdown
                $sharpeRatio = $metrics.performance.sharpe_ratio
                
                # Add to summary
                "$sym,$totalTrades,$winRate,$profit,$returnPct,$maxDrawdown,$sharpeRatio" | Out-File $summaryFile -Append -Encoding UTF8
                
                Write-Host "  Results: Profit=$profit, Win Rate=$winRate, Trades=$totalTrades"
            }
            catch {
                Write-Host "  Error processing metrics for $sym"
            }
        }
        else {
            Write-Host "  No results generated"
        }
        
        # Increment counter
        $current++
        
        Write-Host "------------------------------------------------"
    }
    
    Write-Host "All backtests completed! Results are available in $OutputDir"
    Write-Host "Summary report generated: $summaryFile"
}
else {
    # Run the backtest for a single symbol
    Write-Host "Running backtest for $Symbol. Output will be saved to $OutputDir"
    cargo run --bin run_influx_backtest $Symbol $OutputDir
    Write-Host "Backtest complete! Results are available in $OutputDir"
}