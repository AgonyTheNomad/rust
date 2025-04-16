#!/usr/bin/env python3
"""
Trading Command Utility

A simple utility for sending commands to the running signal generator 
and managing the trading system.
"""

import os
import sys
import json
import time
import argparse
import logging
from datetime import datetime
from typing import Dict, List, Optional, Any

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler()
    ]
)
logger = logging.getLogger("trading_command")

def send_command(command_dir: str, command_type: str, **params) -> bool:
    """Send a command to the signal generator"""
    try:
        os.makedirs(command_dir, exist_ok=True)
        
        command = {
            "type": command_type,
            "timestamp": datetime.now().isoformat(),
            "params": params
        }
        
        timestamp = int(time.time())
        filename = f"{command_type}_{timestamp}.cmd"
        file_path = os.path.join(command_dir, filename)
        
        with open(file_path, 'w') as f:
            json.dump(command, f, indent=2)
            
        logger.info(f"Sent command {command_type} to signal generator")
        return True
        
    except Exception as e:
        logger.error(f"Error sending command: {e}")
        return False

def list_signals(signals_dir: str) -> None:
    """List all signal files in the signals directory"""
    try:
        if not os.path.exists(signals_dir):
            logger.error(f"Signals directory not found: {signals_dir}")
            return
            
        signal_files = [f for f in os.listdir(signals_dir) if f.endswith('.json')]
        
        if not signal_files:
            logger.info("No signal files found")
            return
            
        # Group by symbol
        signals_by_symbol = {}
        
        for filename in signal_files:
            parts = filename.split('_')
            if len(parts) >= 2:
                symbol = parts[0]
                if symbol not in signals_by_symbol:
                    signals_by_symbol[symbol] = []
                signals_by_symbol[symbol].append(filename)
        
        # Print summary
        print(f"\nFound {len(signal_files)} signals across {len(signals_by_symbol)} symbols:\n")
        
        for symbol, files in sorted(signals_by_symbol.items()):
            print(f"{symbol}: {len(files)} signals")
            
            # Get details of most recent signal
            if files:
                newest_file = max(files, key=lambda f: os.path.getmtime(os.path.join(signals_dir, f)))
                try:
                    with open(os.path.join(signals_dir, newest_file), 'r') as f:
                        signal_data = json.load(f)
                        
                    print(f"  Latest: {signal_data.get('position_type')} at ${signal_data.get('price', 0):.2f}")
                    print(f"  Generated: {signal_data.get('timestamp')}")
                    print(f"  Processed: {signal_data.get('processed', False)}\n")
                except Exception as e:
                    print(f"  Error reading signal file: {e}\n")
            
    except Exception as e:
        logger.error(f"Error listing signals: {e}")

def show_status(signals_dir: str, archive_dir: str) -> None:
    """Show current system status"""
    try:
        # Count signals
        active_signals = len([f for f in os.listdir(signals_dir) if f.endswith('.json')]) if os.path.exists(signals_dir) else 0
        archived_signals = len([f for f in os.listdir(archive_dir) if f.endswith('.json')]) if os.path.exists(archive_dir) else 0
        
        # Find newest signal
        newest_time = None
        newest_file = None
        
        if os.path.exists(signals_dir):
            signal_files = [f for f in os.listdir(signals_dir) if f.endswith('.json')]
            
            if signal_files:
                newest_file = max(signal_files, key=lambda f: os.path.getmtime(os.path.join(signals_dir, f)))
                newest_time = datetime.fromtimestamp(os.path.getmtime(os.path.join(signals_dir, newest_file)))
        
        # Print status
        print("\n=== Trading System Status ===\n")
        print(f"Active signals: {active_signals}")
        print(f"Archived signals: {archived_signals}")
        
        if newest_time and newest_file:
            time_since = datetime.now() - newest_time
            print(f"\nNewest signal: {newest_file}")
            print(f"Generated: {newest_time.isoformat()} ({time_since.total_seconds() / 60:.1f} minutes ago)")
        else:
            print("\nNo signals found")
            
        # Look for process status
        print("\n=== Process Status ===\n")
        try:
            # Use ps to find running processes
            import subprocess
            rust_process = subprocess.run(["pgrep", "-f", "signal_generator"], capture_output=True, text=True)
            python_process = subprocess.run(["pgrep", "-f", "hyperliquid_trader.py"], capture_output=True, text=True)
            
            if rust_process.stdout.strip():
                print("Signal Generator: Running")
            else:
                print("Signal Generator: Not running")
                
            if python_process.stdout.strip():
                print("Hyperliquid Trader: Running")
            else:
                print("Hyperliquid Trader: Not running")
        except Exception:
            print("Process status check not available on this platform")
            
        print("\n")
            
    except Exception as e:
        logger.error(f"Error checking status: {e}")

def main():
    parser = argparse.ArgumentParser(description='Trading Command Utility')
    
    subparsers = parser.add_subparsers(dest='command', help='Command to execute')
    
    # Status command
    status_parser = subparsers.add_parser('status', help='Show system status')
    status_parser.add_argument('--signals', type=str, default='../signals', help='Path to signals directory')
    status_parser.add_argument('--archive', type=str, default='../signals/archive', help='Path to archive directory')
    
    # List signals command
    list_parser = subparsers.add_parser('list', help='List signals')
    list_parser.add_argument('--signals', type=str, default='../signals', help='Path to signals directory')
    
    # Pause command
    pause_parser = subparsers.add_parser('pause', help='Pause signal generation')
    pause_parser.add_argument('--commands', type=str, default='../commands', help='Path to commands directory')
    
    # Resume command
    resume_parser = subparsers.add_parser('resume', help='Resume signal generation')
    resume_parser.add_argument('--commands', type=str, default='../commands', help='Path to commands directory')
    
    # Change config command
    config_parser = subparsers.add_parser('config', help='Change configuration')
    config_parser.add_argument('--commands', type=str, default='../commands', help='Path to commands directory')
    config_parser.add_argument('--key', type=str, required=True, help='Configuration key to change')
    config_parser.add_argument('--value', type=str, required=True, help='New value')
    
    args = parser.parse_args()
    
    if args.command == 'status':
        show_status(args.signals, args.archive)
    elif args.command == 'list':
        list_signals(args.signals)
    elif args.command == 'pause':
        send_command(args.commands, 'pause')
    elif args.command == 'resume':
        send_command(args.commands, 'resume')
    elif args.command == 'config':
        send_command(args.commands, 'config', key=args.key, value=args.value)
    else:
        parser.print_help()

if __name__ == "__main__":
    main()