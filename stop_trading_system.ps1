# stop_trading_system.ps1

# Find and stop the signal generator
$signalGeneratorProcesses = Get-Process | Where-Object { $_.CommandLine -match "signal_generator" }
if ($signalGeneratorProcesses) {
    Write-Host "Stopping signal generator..."
    foreach ($process in $signalGeneratorProcesses) {
        $process | Stop-Process -Force
    }
}
else {
    Write-Host "Signal generator not running"
}

# Find and stop the Python trader
$pythonTraderProcesses = Get-Process | Where-Object { $_.ProcessName -eq "python" -and $_.CommandLine -match "hyperliquid_trader.py" }
if ($pythonTraderProcesses) {
    Write-Host "Stopping Hyperliquid trader..."
    foreach ($process in $pythonTraderProcesses) {
        $process | Stop-Process -Force
    }
}
else {
    Write-Host "Hyperliquid trader not running"
}

Write-Host "Trading system stopped"