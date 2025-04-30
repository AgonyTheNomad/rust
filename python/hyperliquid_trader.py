#!/usr/bin/env python3
"""
Hyperliquid Trader - Rust Integration

This script:
1. Monitors the signals directory for new trading signals from the Rust trader
2. Executes trades on Hyperliquid based on signal files
3. Implements position scaling with limit orders and trailing take profits
4. Archives processed signals and logs results
"""

import os
import json
import time
import asyncio
import logging
import argparse
from decimal import Decimal
from pathlib import Path
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Optional, Any

from dotenv import load_dotenv
from eth_account import Account
from hyperliquid.info import Info
from hyperliquid.exchange import Exchange
from hyperliquid.utils import constants

from utils import setup

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[
        logging.StreamHandler(),
        logging.FileHandler("hyperliquid_trader.log")
    ]
)
logger = logging.getLogger("hyperliquid_trader")

class HyperliquidTrader:
    def __init__(self, config_path: str, signals_dir: str, archive_dir: str, commands_dir: str):
        """Initialize the Hyperliquid trader"""
        load_dotenv()
        self.config_path = config_path
        self.signals_dir = Path(signals_dir)
        self.archive_dir = Path(archive_dir)
        self.commands_dir = Path(commands_dir)
        
        # Create directories if they don't exist
        self.signals_dir.mkdir(exist_ok=True)
        self.archive_dir.mkdir(exist_ok=True)
        self.commands_dir.mkdir(exist_ok=True)
        
        # Load config
        with open(config_path, 'r') as f:
            self.config = json.load(f)
        
        # Trading state
        self.open_positions = {}
        self.pending_orders = {}
        self.processed_signals = set()
        self.is_paused = False
        
        # Max age for signals in minutes
        self.max_signal_age = self.config.get('max_signal_age_minutes', 5)
        
        # Max number of positions
        self.max_positions = self.config.get('max_positions', 5)
        
        # Setup Hyperliquid client
        self.address, self.info, self.exchange = setup(skip_ws=False)
        
        # Symbol mapping
        self.symbol_mapping = self.config.get('symbol_mapping', {})
        
        logger.info(f"Hyperliquid trader initialized with config: {config_path}")
        logger.info(f"Using signals directory: {signals_dir}")
        logger.info(f"Using {'TESTNET' if self.config.get('use_testnet') else 'MAINNET'}")
        
    async def start(self):
        """Start the trading loop"""
        logger.info("Starting Hyperliquid trader")
        
        # Print account information
        await self.print_account_info()
        
        # Main loop
        while True:
            try:
                # Check for command files
                await self.check_commands()
                
                if not self.is_paused:
                    # Find and process new signals
                    await self.process_signals()
                    
                    # Check status of open positions
                    await self.check_positions()
                
                # Sleep for a bit
                await asyncio.sleep(1.0)
                
            except Exception as e:
                logger.error(f"Error in trading loop: {e}", exc_info=True)
                await asyncio.sleep(5.0)
    
    async def print_account_info(self):
        """Print account information and balances"""
        try:
            # Get spot balance
            spot = self.info.spot_user_state(self.address)
            if isinstance(spot, dict) and "balances" in spot:
                for b in spot.get("balances", []):
                    if b["coin"] == "USDC":
                        avail = float(b["total"]) - float(b["hold"])
                        logger.info(f"Spot USDC Available: ${avail:.2f}")
                        break
            
            # Get perps information
            state = self.info.user_state(self.address)
            if isinstance(state, dict):
                withdrawable = float(state.get("withdrawable", 0))
                if "crossMarginSummary" in state:
                    cross_margin_summary = state["crossMarginSummary"]
                    account_value = float(cross_margin_summary.get("accountValue", 0))
                    logger.info(f"Account Value: ${account_value:.2f}")
                
                maintenance_margin = float(state.get("crossMaintenanceMarginUsed", 0))
                logger.info(f"Perps Withdrawable: ${withdrawable:.2f}")
                logger.info(f"Maintenance Margin: ${maintenance_margin:.2f}")
                
                # Log positions
                if "assetPositions" in state:
                    for pos in state["assetPositions"]:
                        if isinstance(pos, dict) and "coin" in pos:
                            size = abs(float(pos.get("szi", 0)))
                            if size > 0:
                                side = "LONG" if float(pos.get("szi", 0)) > 0 else "SHORT"
                                entry_px = float(pos.get("entryPx", 0))
                                upnl = float(pos.get("unrealizedPnl", 0))
                                logger.info(f"Open position: {pos['coin']} {side} {size} @ ${entry_px:.2f} (UPNL: ${upnl:.2f})")
            
        except Exception as e:
            logger.error(f"Error getting account info: {e}", exc_info=True)
    
    async def check_commands(self):
        """Check for command files"""
        for cmd_file in self.commands_dir.glob('*.cmd'):
            try:
                with open(cmd_file, 'r') as f:
                    command = json.load(f)
                
                cmd_type = command.get('type')
                logger.info(f"Processing command: {cmd_type}")
                
                if cmd_type == 'stop':
                    logger.info("Received stop command. Exiting...")
                    # Archive the command file
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                    exit(0)
                
                elif cmd_type == 'pause':
                    logger.info("Pausing trading")
                    self.is_paused = True
                    # Archive the command file
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                
                elif cmd_type == 'resume':
                    logger.info("Resuming trading")
                    self.is_paused = False
                    # Archive the command file
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                
                elif cmd_type == 'config':
                    # Update configuration
                    params = command.get('params', {})
                    key = params.get('key')
                    value = params.get('value')
                    
                    if key and value is not None:
                        try:
                            # Convert value to appropriate type
                            if isinstance(self.config.get(key), bool):
                                value = value.lower() == 'true'
                            elif isinstance(self.config.get(key), int):
                                value = int(value)
                            elif isinstance(self.config.get(key), float):
                                value = float(value)
                            
                            # Update config
                            self.config[key] = value
                            logger.info(f"Updated config: {key} = {value}")
                            
                            # Save updated config
                            with open(self.config_path, 'w') as f:
                                json.dump(self.config, f, indent=2)
                        except Exception as e:
                            logger.error(f"Error updating config: {e}")
                    
                    # Archive the command file
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                
                else:
                    logger.warning(f"Unknown command type: {cmd_type}")
                    # Archive the command file
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                
            except Exception as e:
                logger.error(f"Error processing command file {cmd_file}: {e}")
    
    async def process_signals(self):
        """Find and process new signal files"""
        signal_files = list(self.signals_dir.glob('*.json'))
        
        if not signal_files:
            return
        
        # Sort by creation time
        signal_files.sort(key=lambda p: p.stat().st_mtime)
        
        # Get current positions from user_state
        active_symbols = set()
        try:
            # Get positions from user_state
            state = self.info.user_state(self.address)
            if isinstance(state, dict) and "assetPositions" in state:
                for pos in state["assetPositions"]:
                    if isinstance(pos, dict) and "coin" in pos:
                        # Check if position size is non-zero
                        if abs(float(pos.get("szi", 0))) > 0:
                            active_symbols.add(pos["coin"])
            
            # Add positions from our internal tracking
            for pos_id, pos in self.open_positions.items():
                if isinstance(pos, dict) and "symbol" in pos:
                    active_symbols.add(pos["symbol"])
            
            logger.info(f"Symbols with existing positions: {active_symbols}")
            
            # Now process each signal file
            for signal_file in signal_files:
                # Skip files we've already processed
                if signal_file.name in self.processed_signals:
                    continue
                
                try:
                    # Load signal data
                    with open(signal_file, 'r') as f:
                        signal = json.load(f)
                    
                    # Fix for datetime comparison issue
                    signal_time = datetime.fromisoformat(signal['timestamp'].replace('Z', '+00:00'))
                    now_utc = datetime.now(timezone.utc)
                    age_minutes = (now_utc - signal_time).total_seconds() / 60
                    
                    if age_minutes > self.max_signal_age:
                        logger.warning(f"Signal {signal_file.name} is too old ({age_minutes:.1f} min). Archiving.")
                        target = self.archive_dir / signal_file.name
                        signal_file.rename(target)
                        self.processed_signals.add(signal_file.name)
                        continue
                    
                    # Process the signal
                    logger.info(f"Processing signal: {signal_file.name}")
                    
                    # Map the symbol if needed
                    symbol = signal['symbol']
                    exchange_symbol = self.symbol_mapping.get(symbol, symbol)
                    
                    # Check if we already have a position for this symbol
                    if exchange_symbol in active_symbols:
                        logger.warning(f"Already have an open position for {exchange_symbol}. Ignoring signal.")
                        
                        # Mark signal as processed but add a note about why it was ignored
                        signal['processed'] = True
                        signal['ignored_reason'] = "Symbol already has an open position"
                        
                        with open(signal_file, 'w') as f:
                            json.dump(signal, f, indent=2)
                        
                        # Archive the signal file
                        target = self.archive_dir / signal_file.name
                        signal_file.rename(target)
                        self.processed_signals.add(signal_file.name)
                        continue
                    
                    # Check if we're at max positions
                    active_position_count = len(active_symbols)
                    
                    if active_position_count >= self.max_positions:
                        logger.warning(f"Reached maximum number of positions ({self.max_positions}). Skipping signal.")
                        continue
                    
                    # Check position type
                    position_type = signal.get('position_type')
                    is_long = position_type.upper() == 'LONG'
                    
                    # Get price information
                    entry_price = float(signal.get('price', signal.get('entry_price', 0)))
                    
                    # Get take_profit and stop_loss - these could be strings or numbers in the signal
                    take_profit_raw = signal.get('take_profit', 0)
                    take_profit = float(take_profit_raw) if take_profit_raw else 0
                    
                    stop_loss_raw = signal.get('stop_loss', 0)
                    stop_loss = float(stop_loss_raw) if stop_loss_raw else 0
                    
                    # Use signal strength or risk_per_trade from config
                    strength = float(signal.get('strength', 0.8))
                    risk_per_trade = self.config.get('risk_per_trade', 0.01)
                    effective_risk = risk_per_trade * strength
                    
                    # Get position size directly from the signal if available
                    position_size = float(signal.get('size', 0))
                    
                    # Only calculate position size if not provided in the signal
                    if position_size <= 0:
                        try:
                            # Get account information
                            state = self.info.user_state(self.address)
                            account_value = float(state["crossMarginSummary"]["accountValue"])
                            
                            # Calculate risk amount
                            risk_amount = account_value * effective_risk
                            
                            # Calculate risk per contract
                            risk_per_contract = abs(entry_price - stop_loss)
                            if risk_per_contract <= 0:
                                logger.warning(f"Invalid risk per contract: {risk_per_contract}. Using default.")
                                risk_per_contract = entry_price * 0.01  # Use 1% of entry price
                            
                            # Calculate position size in contracts
                            position_size = risk_amount / risk_per_contract
                            
                            # Get minimum trade size for the symbol
                            min_size = 0.001 if exchange_symbol == "BTC" else 0.01  # Default minimums
                            
                            # Apply position limits
                            max_position_size = self.config.get('max_position_size', 1.0)
                            position_size = min(position_size, max_position_size)
                            position_size = max(position_size, min_size)  # Ensure minimum size
                            
                            # Round to appropriate precision based on symbol
                            if exchange_symbol == "BTC":
                                position_size = round(position_size, 3)
                            elif exchange_symbol in ["ETH", "SOL"]:
                                position_size = round(position_size, 2)
                            else:
                                position_size = round(position_size, 1)
                        except Exception as e:
                            logger.error(f"Error calculating position size: {e}")
                            position_size = 0.01  # Default to minimum size on error
                    
                    logger.info(f"Using position size for {exchange_symbol}: {position_size} contracts")
                    
                    # Execute the trade
                    result = await self.execute_trade(
                        signal_id=signal['id'],
                        symbol=exchange_symbol,
                        is_long=is_long,
                        entry_price=entry_price,
                        size=position_size,
                        take_profit=take_profit,
                        stop_loss=stop_loss
                    )
                    
                    if result:
                        # Mark signal as processed in the file
                        signal['processed'] = True
                        with open(signal_file, 'w') as f:
                            json.dump(signal, f, indent=2)
                        
                        # Archive the signal file
                        target = self.archive_dir / signal_file.name
                        signal_file.rename(target)
                        self.processed_signals.add(signal_file.name)
                        logger.info(f"Signal {signal_file.name} processed and archived")
                        
                        # Add to active symbols
                        active_symbols.add(exchange_symbol)
                    else:
                        logger.warning(f"Failed to process signal {signal_file.name} - will retry later")
                    
                except Exception as e:
                    logger.error(f"Error processing signal {signal_file}: {e}", exc_info=True)
        
        except Exception as e:
            logger.error(f"Error in process_signals: {e}", exc_info=True)
    
    async def execute_trade(
        self,
        signal_id,
        symbol,
        is_long,
        entry_price,
        size,
        take_profit,
        stop_loss
    ) -> bool:
        """Execute a trade based on signal parameters"""
        try:
            # Without ticker, we'll use the entry price as our reference
            current_price = entry_price
            logger.info(f"Using signal entry price for {symbol}: ${current_price}")
            
            # Use market order as default since we can't check current price
            use_market = True
            logger.info(f"Executing {'LONG' if is_long else 'SHORT'} for {symbol} using MARKET order")
            logger.info(f"Size: {size}, Entry: ${entry_price}, TP: ${take_profit}, SL: ${stop_loss}")
            
            # Place entry order
            try:
                # Market order
                entry_order = self.exchange.order(
                    symbol, is_long, size, None,
                    {"market": {}},
                    reduce_only=False
                )
                
                logger.info(f"Entry order placed")
                
                # Check response for order ID
                if "response" in entry_order and "data" in entry_order["response"] and "statuses" in entry_order["response"]["data"]:
                    statuses = entry_order["response"]["data"]["statuses"]
                    filled_status = next((s for s in statuses if "filled" in s), None)
                    
                    if filled_status:
                        entry_oid = filled_status["filled"]["oid"]
                        logger.info(f"Entry order filled: {entry_oid}")
                        
                        # Place stop loss order
                        if stop_loss > 0:
                            try:
                                sl_order = self.exchange.order(
                                    symbol, not is_long, size, None,
                                    {"trigger": {"tpsl": "sl", "triggerPx": stop_loss, "isMarket": True}},
                                    reduce_only=True
                                )
                                logger.info(f"Stop loss order placed")
                            except Exception as e:
                                logger.error(f"Error placing stop loss: {e}")
                        
                        # Place take profit order
                        if take_profit > 0:
                            try:
                                tp_order = self.exchange.order(
                                    symbol, not is_long, size, None,
                                    {"trigger": {"tpsl": "tp", "triggerPx": take_profit, "isMarket": True}},
                                    reduce_only=True
                                )
                                logger.info(f"Take profit order placed")
                            except Exception as e:
                                logger.error(f"Error placing take profit: {e}")
                        
                        # Store position information
                        position_id = f"{symbol}_{entry_oid}"
                        self.open_positions[position_id] = {
                            "signal_id": signal_id,
                            "symbol": symbol,
                            "is_long": is_long,
                            "entry_price": entry_price,
                            "current_size": size,
                            "take_profit": take_profit,
                            "stop_loss": stop_loss,
                            "entry_time": time.time()
                        }
                        
                        return True
                    else:
                        logger.error("No filled status found in order response")
                        return False
                else:
                    logger.warning("Unexpected response format from order placement")
                    return False
                
            except Exception as e:
                logger.error(f"Error placing entry order: {e}")
                return False
            
        except Exception as e:
            logger.error(f"Error executing trade: {e}", exc_info=True)
            return False
    
    async def check_positions(self):
        """Check status of open positions and pending orders"""
        try:
            # Clean up expired positions
            current_time = time.time()
            positions_to_remove = []
            
            for pos_id, pos_info in self.open_positions.items():
                # Check if position is older than 24 hours
                if current_time - pos_info.get("entry_time", 0) > 86400:  # 24 hours
                    positions_to_remove.append(pos_id)
            
            # Remove expired positions
            for pos_id in positions_to_remove:
                logger.info(f"Removing expired position tracking for {pos_id}")
                self.open_positions.pop(pos_id, None)
            
            # Check for pending orders
            pending_orders_to_remove = []
            
            for oid, order_info in self.pending_orders.items():
                # Check if order is older than 10 minutes
                if current_time - order_info.get("timestamp", 0) > 600:  # 10 minutes
                    pending_orders_to_remove.append(oid)
            
            # Remove expired pending orders
            for oid in pending_orders_to_remove:
                logger.info(f"Removing expired pending order {oid}")
                self.pending_orders.pop(oid, None)
                
        except Exception as e:
            logger.error(f"Error checking positions: {e}", exc_info=True)


def parse_args():
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(description="Hyperliquid Trader - Rust Integration")
    
    parser.add_argument(
        "--config",
        type=str,
        default="./python/config.json",
        help="Path to configuration file"
    )
    
    parser.add_argument(
        "--signals",
        type=str,
        default="./signals",
        help="Path to signals directory"
    )
    
    parser.add_argument(
        "--archive",
        type=str,
        default="./signals/archive",
        help="Path to archive directory"
    )
    
    parser.add_argument(
        "--commands",
        type=str,
        default="./commands",
        help="Path to commands directory"
    )
    
    return parser.parse_args()

async def main():
    """Main function"""
    args = parse_args()
    
    # Create trader
    trader = HyperliquidTrader(
        config_path=args.config,
        signals_dir=args.signals,
        archive_dir=args.archive,
        commands_dir=args.commands
    )
    
    # Start trading
    await trader.start()

if __name__ == "__main__":
    # Run the main function
    asyncio.run(main())