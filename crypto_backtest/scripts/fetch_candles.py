from influxdb_client import InfluxDBClient
from datetime import datetime, timedelta
import json
import os
import sys

url = "http://0.0.0.0:8086"
token = "Xu0vYUoLT_lAA02JKERHPS5jl02cN4YA76AJzZMH7FeApVKksrrcafLm3WVcZJj6VcZm53oUgR6PE8HMq39IpQ=="
org = "ValhallaVault"
bucket = "hyper_candles"

def fetch_candles(symbol="BTC", start_time=None, end_time=None):
    if not start_time:
        start_time = datetime.now() - timedelta(days=1)
    if not end_time:
        end_time = datetime.now()

    print(f"Fetching {symbol} candles from {start_time} to {end_time}")
    
    client = InfluxDBClient(url=url, token=token, org=org)
    query_api = client.query_api()

    query = f'''from(bucket: "{bucket}")
        |> range(start: -{int((datetime.now() - start_time).total_seconds())}s)
        |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "{symbol}")
        |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")'''

    print(f"Executing query: {query}")
    result = query_api.query(query)

    candles = []
    for table in result:
        for record in table.records:
            candle = {
                'time': record.get_time().isoformat(),  # Convert to ISO8601 string directly
                'open': record.values.get('open'),
                'high': record.values.get('high'),
                'low': record.values.get('low'),
                'close': record.values.get('close'),
                'volume': record.values.get('volume'),
                'num_trades': record.values.get('num_trades')
            }
            candles.append(candle)

    client.close()
    print(f"Fetched {len(candles)} candles")
    return candles

def save_candles(symbol, candles):
    # Create directory structure
    base_dir = os.path.join("data", symbol.lower())
    data_dir = os.path.join(base_dir, "data")
    os.makedirs(data_dir, exist_ok=True)

    # Generate filename with timestamp
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    filename = f"candles_{timestamp}.json"
    filepath = os.path.join(data_dir, filename)

    # Save candles
    with open(filepath, 'w') as f:
        json.dump(candles, f, indent=2)
    
    print(f"Saved {len(candles)} candles to {filepath}")
    return filepath

def main():
    # Allow symbol to be passed as command line argument
    symbol = sys.argv[1] if len(sys.argv) > 1 else "BTC"
    
    try:
        print(f"Fetching data for {symbol}...")
        candles = fetch_candles(symbol)
        filepath = save_candles(symbol, candles)
        
        # Create a symlink or copy to latest.json
        latest_path = os.path.join("data", symbol.lower(), "data", "latest.json")
        if os.path.exists(latest_path):
            os.remove(latest_path)
        
        # Save to latest.json as well
        with open(latest_path, 'w') as f:
            json.dump(candles, f, indent=2)
        
        print(f"Also saved to {latest_path}")
        
    except Exception as e:
        print(f"Error: {str(e)}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()