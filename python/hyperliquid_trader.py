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
from datetime import datetime, timedelta
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
            spot = await self.info.spot_user_state(self.address)
            for b in spot.get("balances", []):
                if b["coin"] == "USDC":
                    avail = float(b["total"]) - float(b["hold"])
                    logger.info(f"Spot USDC Available: ${avail:.2f}")
                    break
            else:
                logger.info("Spot USDC Available: $0.00")
            
            # Get perps information
            state = await self.info.user_state(self.address)
            withdrawable = float(state.get("withdrawable", 0))
            cross_margin_summary = state["crossMarginSummary"]
            account_value = float(cross_margin_summary["accountValue"])
            maintenance_margin = float(state.get("crossMaintenanceMarginUsed", 0))
            ratio = maintenance_margin / account_value if account_value else 0.0
            
            logger.info(f"Perps Withdrawable: ${withdrawable:.2f}")
            logger.info(f"Account Value: ${account_value:.2f}")
            logger.info(f"Maintenance Margin: ${maintenance_margin:.2f}")
            logger.info(f"Cross-Margin Ratio: {ratio:.2%}")
            
            # Get open positions
            positions = await self.info.user_positions(self.address)
            for pos in positions:
                if abs(float(pos.get("szi", 0))) > 0:
                    side = "LONG" if float(pos.get("szi", 0)) > 0 else "SHORT"
                    size = abs(float(pos.get("szi", 0)))
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
        
        # Get current positions to check for symbols with open positions
        current_positions = await self.info.user_positions(self.address)
        
        # Create a set of symbols that already have open positions
        symbols_with_positions = set()
        for pos in current_positions:
            if abs(float(pos.get("szi", 0))) > 0:
                # Add the symbol to our set
                symbols_with_positions.add(pos["coin"])
                
        logger.info(f"Symbols with existing positions: {symbols_with_positions}")
        
        for signal_file in signal_files:
            # Skip files we've already processed
            if signal_file.name in self.processed_signals:
                continue
            
            try:
                # Load signal data
                with open(signal_file, 'r') as f:
                    signal = json.load(f)
                
                # Check if signal is too old
                signal_time = datetime.fromisoformat(signal['timestamp'].replace('Z', '+00:00'))
                age_minutes = (datetime.now() - signal_time).total_seconds() / 60
                
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
                if exchange_symbol in symbols_with_positions:
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
                active_positions = len(symbols_with_positions)
                
                if active_positions >= self.max_positions:
                    logger.warning(f"Reached maximum number of positions ({self.max_positions}). Skipping signal.")
                    continue
                
                # Check position type
                position_type = signal.get('position_type')
                is_long = position_type.upper() == 'LONG'
                
                # Get price information
                entry_price = float(signal.get('price', signal.get('entry_price', 0)))
                take_profit = float(signal.get('take_profit', 0))
                stop_loss = float(signal.get('stop_loss', 0))
                
                # Scaling parameters from Rust trader
                # These might not be in every signal, so use get() with defaults
                limit1_price = float(signal.get('limit1_price', 0)) if signal.get('limit1_price') else None
                limit2_price = float(signal.get('limit2_price', 0)) if signal.get('limit2_price') else None
                limit1_size = float(signal.get('limit1_size', 0))
                limit2_size = float(signal.get('limit2_size', 0))
                new_tp1 = float(signal.get('new_tp1', 0)) if signal.get('new_tp1') else None
                new_tp2 = float(signal.get('new_tp2', 0)) if signal.get('new_tp2') else None
                
                # If limits not provided, calculate based on Fibonacci levels
                # These calculations are based on the Rust trader's fibonacci.rs
                if not limit1_price and not limit2_price:
                    if is_long:
                        # For long positions in a pullback strategy:
                        # - Limit orders are set lower than entry
                        # - Take profit is set higher than entry
                        range_value = abs(entry_price - stop_loss)
                        limit1_price = entry_price - (0.5 * range_value) 
                        limit2_price = entry_price - (0.786 * range_value)
                        limit1_size = 0.5 * float(self.config.get('position_size', 0.01))
                        limit2_size = 0.8 * float(self.config.get('position_size', 0.01))
                        new_tp1 = entry_price + (0.4 * range_value)
                        new_tp2 = entry_price + (0.6 * range_value)
                    else:
                        # For short positions in a pullback strategy:
                        # - Limit orders are set higher than entry
                        # - Take profit is set lower than entry
                        range_value = abs(stop_loss - entry_price)
                        limit1_price = entry_price + (0.5 * range_value)
                        limit2_price = entry_price + (0.786 * range_value)
                        limit1_size = 0.5 * float(self.config.get('position_size', 0.01))
                        limit2_size = 0.8 * float(self.config.get('position_size', 0.01))
                        new_tp1 = entry_price - (0.4 * range_value)
                        new_tp2 = entry_price - (0.6 * range_value)
                
                # Calculate position size based on risk
                position_size = await self.calculate_position_size(
                    exchange_symbol, 
                    entry_price,
                    stop_loss,
                    is_long
                )
                
                # Execute the trade
                result = await self.execute_trade(
                    signal_id=signal['id'],
                    symbol=exchange_symbol,
                    is_long=is_long,
                    entry_price=entry_price,
                    size=position_size,
                    take_profit=take_profit,
                    stop_loss=stop_loss,
                    limit1_price=limit1_price,
                    limit2_price=limit2_price,
                    limit1_size=limit1_size,
                    limit2_size=limit2_size,
                    new_tp1=new_tp1,
                    new_tp2=new_tp2
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
                    
                    # Add to symbols_with_positions if we placed a trade
                    symbols_with_positions.add(exchange_symbol)
                else:
                    logger.warning(f"Failed to process signal {signal_file.name} - will retry later")
                
            except Exception as e:
                logger.error(f"Error processing signal {signal_file}: {e}", exc_info=True)
    
    async def calculate_position_size(self, symbol: str, entry_price: float, stop_loss: float, is_long: bool) -> float:
        """Calculate position size based on risk parameters"""
        try:
            # Get account value
            account_state = await self.info.user_state(self.address)
            account_value = float(account_state["crossMarginSummary"]["accountValue"])
            
            # Get risk per trade from config
            risk_percent = self.config.get('risk_per_trade', 0.01)  # Default 1%
            risk_amount = account_value * risk_percent
            
            # Calculate risk per contract
            risk_per_contract = abs(entry_price - stop_loss)
            if risk_per_contract <= 0:
                logger.warning(f"Invalid risk per contract: {risk_per_contract}. Using default.")
                risk_per_contract = entry_price * 0.01  # Use 1% of entry price
            
            # Calculate position size in contracts
            position_size = risk_amount / risk_per_contract
            
            # Get minimum trade size for the symbol
            min_size = 0.001 if symbol == "BTC" else 0.01  # Default minimums
            
            # Apply position limits
            max_position_size = self.config.get('max_position_size', 1.0)
            position_size = min(position_size, max_position_size)
            position_size = max(position_size, min_size)  # Ensure minimum size
            
            # Round to appropriate precision based on symbol
            if symbol == "BTC":
                position_size = round(position_size, 3)
            elif symbol in ["ETH", "SOL"]:
                position_size = round(position_size, 2)
            else:
                position_size = round(position_size, 1)
            
            logger.info(f"Calculated position size for {symbol}: {position_size} contracts")
            return position_size
            
        except Exception as e:
            logger.error(f"Error calculating position size: {e}")
            return 0.01  # Return minimum size on error
    
    async def execute_trade(
        self,
        signal_id,
        symbol,
        is_long,
        entry_price,
        size,
        take_profit,
        stop_loss,
        limit1_price=None,
        limit2_price=None,
        limit1_size=0,
        limit2_size=0,
        new_tp1=None,
        new_tp2=None,
    ) -> bool:
        """Execute a trade based on signal parameters"""
        try:
            # Check current price to determine order type
            ticker = await self.info.ticker(symbol)
            current_price = float(ticker["midPrice"])
            
            # Determine if we should use market or limit order for entry
            use_market = False
            price_diff_percent = abs(entry_price - current_price) / current_price
            
            if is_long:
                if current_price <= entry_price:
                    use_market = True
            else:  # Short
                if current_price >= entry_price:
                    use_market = True
            
            # Use market if price is close enough (within 0.1%)
            if price_diff_percent < 0.001:
                use_market = True
            
            logger.info(f"Executing {'LONG' if is_long else 'SHORT'} for {symbol} at {'MARKET' if use_market else 'LIMIT'}")
            logger.info(f"Size: {size}, Entry: ${entry_price}, TP: ${take_profit}, SL: ${stop_loss}")
            
            if limit1_price and limit1_size > 0:
                logger.info(f"Limit1: {limit1_size} @ ${limit1_price}, New TP1: ${new_tp1}")
            
            if limit2_price and limit2_size > 0:
                logger.info(f"Limit2: {limit2_size} @ ${limit2_price}, New TP2: ${new_tp2}")
            
            # Place entry order
            if use_market:
                # Market order
                entry_order = self.exchange.order(
                    symbol, is_long, size, None,
                    {"market": {}},
                    reduce_only=False
                )
            else:
                # Limit order
                entry_order = self.exchange.order(
                    symbol, is_long, size, entry_price,
                    {"limit": {"tif": "Gtc"}},
                    reduce_only=False
                )
            
            logger.info(f"Entry order response: {entry_order}")
            
            # Check for immediate fill or resting order
            statuses = entry_order["response"]["data"]["statuses"]
            filled_status = next((s for s in statuses if "filled" in s), None)
            resting_status = next((s for s in statuses if "resting" in s), None)
            
            entry_oid = None
            
            if filled_status:
                entry_oid = filled_status["filled"]["oid"]
                logger.info(f"Entry order filled immediately: {entry_oid}")
                filled_immediately = True
            elif resting_status:
                entry_oid = resting_status["resting"]["oid"]
                logger.info(f"Entry order resting: {entry_oid}")
                filled_immediately = False
                
                # Store pending order
                self.pending_orders[entry_oid] = {
                    "signal_id": signal_id,
                    "symbol": symbol,
                    "is_long": is_long,
                    "entry_price": entry_price,
                    "size": size,
                    "take_profit": take_profit,
                    "stop_loss": stop_loss,
                    "limit1_price": limit1_price,
                    "limit2_price": limit2_price,
                    "limit1_size": limit1_size,
                    "limit2_size": limit2_size,
                    "new_tp1": new_tp1,
                    "new_tp2": new_tp2,
                    "timestamp": time.time()
                }
                
                # We'll wait for the order to fill
                return True
            else:
                logger.error("No order ID found in entry response")
                return False
            
            # If order filled immediately, place TP/SL orders
            if filled_immediately:
                # Place stop loss order
                sl_order = self.exchange.order(
                    symbol, not is_long, size, None,
                    {"trigger": {"tpsl": "sl", "triggerPx": stop_loss, "isMarket": True}},
                    reduce_only=True
                )
                logger.info(f"Stop loss order placed: {sl_order}")
                
                # Place take profit order
                tp_order = self.exchange.order(
                    symbol, not is_long, size, None,
                    {"trigger": {"tpsl": "tp", "triggerPx": take_profit, "isMarket": True}},
                    reduce_only=True
                )
                logger.info(f"Take profit order placed: {tp_order}")
                
                # Place limit orders if specified
                if limit1_price and limit1_size > 0:
                    limit1_order = self.exchange.order(
                        symbol, is_long, limit1_size, limit1_price,
                        {"limit": {"tif": "Gtc"}},
                        reduce_only=False
                    )
                    logger.info(f"Limit1 order placed: {limit1_order}")
                
                if limit2_price and limit2_size > 0:
                    limit2_order = self.exchange.order(
                        symbol, is_long, limit2_size, limit2_price,
                        {"limit": {"tif": "Gtc"}},
                        reduce_only=False
                    )
                    logger.info(f"Limit2 order placed: {limit2_order}")
                
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
                    "limit1_price": limit1_price,
                    "limit2_price": limit2_price,
                    "limit1_size": limit1_size,
                    "limit2_size": limit2_size,
                    "limit1_hit": False,
                    "limit2_hit": False,
                    "new_tp1": new_tp1,
                    "new_tp2": new_tp2,
                    "entry_time": time.time(),
                    "tp_oid": None,
                    "sl_oid": None,
                    "limit1_oid": None,
                    "limit2_oid": None
                }
            
            return True
            
        except Exception as e:
            logger.error(f"Error executing trade: {e}", exc_info=True)
            return False
    
    async def check_positions(self):
        """Check status of open positions and pending orders"""
        try:
            # Check pending orders first
            for oid, order_info in list(self.pending_orders.items()):
                # Skip orders that are too fresh (< 2 seconds)
                if time.time() - order_info["timestamp"] < 2:
                    continue
                
                # Query order status
                try:
                    order_status = await self.info.query_order_by_oid(self.address, oid)
                    status = order_status.get("order", {}).get("status")
                    
                    if status == "filled":
                        logger.info(f"Pending order {oid} has been filled")
                        
                        # Extract order info
                        symbol = order_info["symbol"]
                        is_long = order_info["is_long"]
                        size = order_info["size"]
                        take_profit = order_info["take_profit"]
                        stop_loss = order_info["stop_loss"]
                        limit1_price = order_info.get("limit1_price")
                        limit1_size = order_info.get("limit1_size", 0)
                        limit2_price = order_info.get("limit2_price")
                        limit2_size = order_info.get("limit2_size", 0)
                        new_tp1 = order_info.get("new_tp1")
                        new_tp2 = order_info.get("new_tp2")
                        
                        # Place stop loss order
                        sl_order = self.exchange.order(
                            symbol, not is_long, size, None,
                            {"trigger": {"tpsl": "sl", "triggerPx": stop_loss, "isMarket": True}},
                            reduce_only=True
                        )
                        logger.info(f"Stop loss order placed: {sl_order}")
                        
                        # Place take profit order
                        tp_order = self.exchange.order(
                            symbol, not is_long, size, None,
                            {"trigger": {"tpsl": "tp", "triggerPx": take_profit, "isMarket": True}},
                            reduce_only=True
                        )
                        logger.info(f"Take profit order placed: {tp_order}")
                        
                        # Extract order IDs
                        sl_oid = None
                        tp_oid = None
                        
                        for status in sl_order["response"]["data"]["statuses"]:
                            if "resting" in status:
                                sl_oid = status["resting"]["oid"]
                            elif "triggered" in status:
                                sl_oid = status["triggered"]["oid"]
                        
                        for status in tp_order["response"]["data"]["statuses"]:
                            if "resting" in status:
                                tp_oid = status["resting"]["oid"]
                            elif "triggered" in status:
                                tp_oid = status["triggered"]["oid"]
                        
                        # Place limit orders if specified
                        limit1_oid = None
                        if limit1_price and limit1_size > 0:
                            limit1_order = self.exchange.order(
                                symbol, is_long, limit1_size, limit1_price,
                                {"limit": {"tif": "Gtc"}},
                                reduce_only=False
                            )
                            logger.info(f"Limit1 order placed: {limit1_order}")
                            
                            for status in limit1_order["response"]["data"]["statuses"]:
                                if "resting" in status:
                                    limit1_oid = status["resting"]["oid"]
                        
                        limit2_oid = None
                        if limit2_price and limit2_size > 0:
                            limit2_order = self.exchange.order(
                                symbol, is_long, limit2_size, limit2_price,
                                {"limit": {"tif": "Gtc"}},
                                reduce_only=False
                            )
                            logger.info(f"Limit2 order placed: {limit2_order}")
                            
                            for status in limit2_order["response"]["data"]["statuses"]:
                                if "resting" in status:
                                    limit2_oid = status["resting"]["oid"]
                        
                        # Store position information
                        position_id = f"{symbol}_{oid}"
                        self.open_positions[position_id] = {
                            "signal_id": order_info["signal_id"],
                            "symbol": symbol,
                            "is_long": is_long,
                            "entry_price": order_info["entry_price"],
                            "current_size": size,
                            "take_profit": take_profit,
                            "stop_loss": stop_loss,
                            "limit1_price": limit1_price,
                            "limit2_price": limit2_price,
                            "limit1_size": limit1_size,
                            "limit2_size": limit2_size,
                            "limit1_hit": False,
                            "limit2_hit": False,
                            "new_tp1": new_tp1,
                            "new_tp2": new_tp2,
                            "entry_time": time.time(),
                            "tp_oid": tp_oid,
                            "sl_oid": sl_oid,
                            "limit1_oid": limit1_oid,
                            "limit2_oid": limit2_oid
                        }
                        
                        # Remove from pending orders
                        del self.pending_orders[oid]
                    
                    elif status == "canceled" or status == "expired":
                        logger.info(f"Pending order {oid} was {status}")
                        del self.pending_orders[oid]
                
                except Exception as e:
                    logger.error(f"Error checking order {oid}: {e}")
            
            # Get current positions and prices
            positions = await self.info.user_positions(self.address)
            open_orders = await self.info.open_orders(self.address)
            
            # Create a map of positions by symbol
            position_by_symbol = {}
            for pos in positions:
                if abs(float(pos.get("szi", 0))) > 0:
                    position_by_symbol[pos["coin"]] = pos
            
            # Check each position
            for position_id, position in list(self.open_positions.items()):
                symbol = position["symbol"]
                
                # If position no longer in exchange positions, it was closed
                if symbol not in position_by_symbol:
                    logger.info(f"Position {position_id} for {symbol} has been closed")
                    del self.open_positions[position_id]
                    continue
                
                # Check current price
                ticker = await self.info.ticker(symbol)
                current_price = float(ticker["midPrice"])
                
                # Calculate current size from exchange data
                exchange_position = position_by_symbol[symbol]
                current_size = abs(float(exchange_position.get("szi", 0)))
                expected_size = position["current_size"]
                
                # Check if limit orders were hit
                limit1_hit = position.get("limit1_hit", False)
                limit2_hit = position.get("limit2_hit", False)
                
                # For limit1 order
                if not limit1_hit and position.get("limit1_price") and position.get("limit1_size", 0) > 0:
                    limit1_price = position["limit1_price"]
                    is_long = position["is_long"]
                    
                    # Check if price hit the limit level
                    price_hit_limit1 = (is_long and current_price <= limit1_price) or (not is_long and current_price >= limit1_price)
                    
                    # Or check if size increased (limit order filled)
                    size_increased = current_size > expected_size
                    
                    if price_hit_limit1 or size_increased:
                        logger.info(f"Limit1 order hit for {symbol} at {current_price}")
                        position["limit1_hit"] = True
                        position["current_size"] += position["limit1_size"]
                        
                        # Update expected size
                        expected_size = position["current_size"]
                        
                        # Cancel old TP order if needed
                        if position.get("tp_oid"):
                            try:
                                self.exchange.cancel_order(symbol, position["tp_oid"])
                                logger.info(f"Canceled old TP order {position['tp_oid']}")
                            except Exception as e:
                                logger.error(f"Error canceling TP order: {e}")
                        
                        # Place new TP order with adjusted price
                        if position.get("new_tp1"):
                            new_tp = position["new_tp1"]
                            position["take_profit"] = new_tp
                            
                            tp_order = self.exchange.order(
                                symbol, not is_long, expected_size, None,
                                {"trigger": {"tpsl": "tp", "triggerPx": new_tp, "isMarket": True}},
                                reduce_only=True
                            )
                            logger.info(f"New TP1 order placed at {new_tp}: {tp_order}")
                            
                            # Extract order ID
                            for status in tp_order["response"]["data"]["statuses"]:
                                if "resting" in status:
                                    position["tp_oid"] = status["resting"]["oid"]
                                elif "triggered" in status:
                                    position["tp_oid"] = status["triggered"]["oid"]
                
                # For limit2 order - only check if limit1 has been hit
                if limit1_hit and not limit2_hit and position.get("limit2_price") and position.get("limit2_size", 0) > 0:
                    limit2_price = position["limit2_price"]
                    is_long = position["is_long"]
                    
                    # Check if price hit the limit level
                    price_hit_limit2 = (is_long and current_price <= limit2_price) or (not is_long and current_price >= limit2_price)
                    
                    # Or check if size increased further (limit order filled)
                    size_increased = current_size > expected_size
                    
                    if price_hit_limit2 or size_increased:
                        logger.info(f"Limit2 order hit for {symbol} at {current_price}")
                        position["limit2_hit"] = True
                        position["current_size"] += position["limit2_size"]
                        
                        # Cancel old TP order if needed
                        if position.get("tp_oid"):
                            try:
                                self.exchange.cancel_order(symbol, position["tp_oid"])
                                logger.info(f"Canceled old TP order {position['tp_oid']}")
                            except Exception as e:
                                logger.error(f"Error canceling TP order: {e}")
                        
                        # Place new TP order with adjusted price
                        if position.get("new_tp2"):
                            new_tp = position["new_tp2"]
                            position["take_profit"] = new_tp
                            
                            tp_order = self.exchange.order(
                                symbol, not is_long, position["current_size"], None,
                                {"trigger": {"tpsl": "tp", "triggerPx": new_tp, "isMarket": True}},
                                reduce_only=True
                            )
                            logger.info(f"New TP2 order placed at {new_tp}: {tp_order}")
                            
                            # Extract order ID
                            for status in tp_order["response"]["data"]["statuses"]:
                                if "resting" in status:
                                    position["tp_oid"] = status["resting"]["oid"]
                                elif "triggered" in status:
                                    position["tp_oid"] = status["triggered"]["oid"]
        
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