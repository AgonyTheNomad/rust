# trading_command.ps1

# Just a wrapper to call the Python script
$argString = $args -join " "
python trading_command.py $argString