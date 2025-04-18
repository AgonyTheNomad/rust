#!/usr/bin/env python3
"""
Hyperliquid Test Signal Creator

Creates a BTC test signal file compatible with the Hyperliquid SDK integration
to verify that hyperliquid_trader.py is working correctly.
"""

import os
import json
import time
from datetime import datetime, timezone
import uuid

# Configuration - adjust paths as needed for your project structure
SIGNALS_DIR = "./signals"

# Create the signals directory if it doesn't exist
os.makedirs(SIGNALS_DIR, exist_ok=True)

# Create a test signal for BTC
def create_test_signal():
    # Generate a unique ID
    signal_id = str(uuid.uuid4())
    
    # Current timestamp in ISO format with timezone
    timestamp = datetime.now(timezone.utc).isoformat()
    
    # Signal parameters
    entry_price = 80000.0  # Entry price for BTC
    stop_loss = 72000.0    # Stop loss level
    take_profit = 92000.0  # Take profit level
    
    # Create the signal data object
    signal_data = {
        "id": signal_id,
        "symbol": "BTC",  # This will be mapped to the correct Hyperliquid asset name
        "timestamp": timestamp,
        "position_type": "Long",
        "price": entry_price,
        "reason": "Test signal for Hyperliquid SDK integration",
        "strength": 0.9,
        "take_profit": take_profit,
        "stop_loss": stop_loss,
        "processed": False,
        "metadata": {
            "test_signal": True,
            "risk_reward_ratio": (take_profit - entry_price) / (entry_price - stop_loss),
            "stop_loss_percent": (entry_price - stop_loss) / entry_price * 100,
            "take_profit_percent": (take_profit - entry_price) / entry_price * 100
        }
    }
    
    # Create a filename with timestamp for uniqueness
    unix_timestamp = int(time.time())
    filename = f"BTC_Long_{unix_timestamp}.json"
    file_path = os.path.join(SIGNALS_DIR, filename)
    
    # Write the signal to file with nice formatting
    with open(file_path, 'w') as f:
        json.dump(signal_data, f, indent=2)
    
    return file_path, signal_data

# Create the test signal
signal_path, signal_data = create_test_signal()

# Display information about the created signal
print("\n==== TEST SIGNAL CREATED ====")
print(f"File: {signal_path}")
print(f"Symbol: {signal_data['symbol']}")
print(f"Position: {signal_data['position_type']}")
print(f"Entry Price: ${signal_data['price']}")
print(f"Stop Loss: ${signal_data['stop_loss']} (${signal_data['price'] - signal_data['stop_loss']} away, {signal_data['metadata']['stop_loss_percent']:.2f}%)")
print(f"Take Profit: ${signal_data['take_profit']} (${signal_data['take_profit'] - signal_data['price']} away, {signal_data['metadata']['take_profit_percent']:.2f}%)")
print(f"Risk/Reward: {signal_data['metadata']['risk_reward_ratio']:.2f}")
print("\n==== NEXT STEPS ====")
print("1. Check your trader logs to verify signal processing:")
print("   tail -f logs/hyperliquid_trader.log")
print("2. Verify the signal file gets updated with 'processed: true'")
print("3. If in dry-run mode, no actual trade will be executed\n")