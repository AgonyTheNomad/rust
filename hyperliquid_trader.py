#!/usr/bin/env python3
"""
Hyperliquid Trader - Rust Integration

This script:
1. Monitors the signals directory for new trading signals from the Rust trader
2. Executes trades on Hyperliquid based on signal files
3. Implements position scaling with limit orders and trailing take profits
4. Archives processed signals and logs results
5. Writes account information to a file for the Rust trader
"""

import os
import json
import time
import math
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
        self.account_info_file = Path("./account_info.json")
        
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
        self.open_orders = {}  # Track open unfilled orders by symbol
        self.is_paused = False
        
        # Tick sizes for symbols
        self.tick_sizes = {}
        
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
        
        # Fetch asset metadata including tick sizes
        await self.fetch_asset_metadata()
        
        # Print account information and update account info file
        await self.update_account_info()
        
        # Track last account update time
        last_account_update = time.time()
        
        # Main loop
        while True:
            try:
                # Check for command files
                await self.check_commands()
                
                # Update account info every 60 seconds
                current_time = time.time()
                if current_time - last_account_update > 60:
                    await self.update_account_info()
                    last_account_update = current_time
                
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
    
    async def update_account_info(self):
        """Update account information and write to a file for Rust to read"""
        try:
            # Get perps information
            state = self.info.user_state(self.address)
            
            # Display basic account info
            if isinstance(state, dict):
                withdrawable = float(state.get("withdrawable", 0))
                account_value = 0.0
                if "crossMarginSummary" in state:
                    cross_margin_summary = state["crossMarginSummary"]
                    account_value = float(cross_margin_summary.get("accountValue", 0))
                    logger.info(f"Account Value: ${account_value:.2f}")
                
                maintenance_margin = float(state.get("crossMaintenanceMarginUsed", 0))
                logger.info(f"Perps Withdrawable: ${withdrawable:.2f}")
                logger.info(f"Maintenance Margin: ${maintenance_margin:.2f}")
                
                # Create account info object to write to file
                account_info = {
                    "balance": account_value,
                    "available_margin": withdrawable,
                    "used_margin": maintenance_margin,
                    "timestamp": time.time(),
                    "positions": []
                }
                
                # Add position information
                if "assetPositions" in state:
                    for pos in state["assetPositions"]:
                        if isinstance(pos, dict) and "coin" in pos:
                            size = float(pos.get("szi", 0))
                            if abs(size) > 0:
                                entry_px = float(pos.get("entryPx", 0))
                                upnl = float(pos.get("unrealizedPnl", 0))
                                position = {
                                    "symbol": pos["coin"],
                                    "size": abs(size),
                                    "entry_price": entry_px,
                                    "side": "LONG" if size > 0 else "SHORT",
                                    "unrealized_pnl": upnl,
                                    "mark_price": float(pos.get("markPx", entry_px))
                                }
                                account_info["positions"].append(position)
                                logger.info(f"Open position: {pos['coin']} {position['side']} {abs(size)} @ ${entry_px:.2f} (UPNL: ${upnl:.2f})")
                
                # Write to file for Rust trader to read
                with open(self.account_info_file, 'w') as f:
                    json.dump(account_info, f, indent=2)
                
                logger.info(f"Updated account info file. Balance: ${account_value:.2f}")
                
                return account_info
            
        except Exception as e:
            logger.error(f"Error updating account info: {e}", exc_info=True)
            return None
    
    async def fetch_asset_metadata(self):
        """Fetch and store metadata for all assets including tick sizes"""
        try:
            # Get metadata from the API
            meta = self.info.meta()
            universe = meta.get("universe", [])
            
            # First, try to extract tick sizes directly
            for asset in universe:
                symbol = asset.get("name")
                if not symbol:
                    continue
                
                # Try to get tick size from various possible fields
                tick_size = None
                if "tickSize" in asset:
                    tick_size = float(asset.get("tickSize"))
                elif "px_step" in asset:
                    tick_size = float(asset.get("px_step"))
                elif "step" in asset:
                    tick_size = float(asset.get("step"))
                
                # If no explicit tick size, use decimals to calculate
                if tick_size is None:
                    # Use known common values for major coins
                    if symbol == "BTC":
                        tick_size = 1  # BTC typically uses 0.1
                    elif symbol == "ETH":
                        tick_size = 0.1  # ETH, SOL typically use 0.01
                    else:
                        # Default to 0.001 or use szDecimals if available
                        sz_decimals = asset.get("szDecimals", 3)
                        tick_size = 10 ** -sz_decimals
                
                if tick_size:
                    self.tick_sizes[symbol] = tick_size
            
            if not self.tick_sizes:
                # If we couldn't extract tick sizes, use defaults
                self.set_default_tick_sizes()
            
            # Apply critical overrides regardless of what the API returned
            # This ensures BTC and MKR always use the correct tick sizes
            manual_overrides = {
                "BTC": 1.0,   # Force BTC to whole dollars
                "MKR": 0.1    # Force MKR to 0.1 increments
            }
            self.tick_sizes.update(manual_overrides)
            
            logger.info(f"Loaded tick sizes for {len(self.tick_sizes)} symbols:")
            for symbol, tick in sorted(self.tick_sizes.items()):
                logger.info(f"  {symbol}: {tick}")
            
        except Exception as e:
            logger.error(f"Error fetching asset metadata: {e}", exc_info=True)
            # Use defaults if API fetch failed
            self.set_default_tick_sizes()
            
            # Even after setting defaults, make sure to apply critical overrides
            manual_overrides = {
                "BTC": 1.0,   # Force BTC to whole dollars
                "MKR": 0.1    # Force MKR to 0.1 increments
            }
            self.tick_sizes.update(manual_overrides)
    
    def set_default_tick_sizes(self):
        """Set default tick sizes for common symbols"""
        default_tick_sizes = {
            "BTC": 1.0,  # BTC uses whole dollar increments
            "ETH": 0.1,
            "SOL": 0.01,
            "APT": 0.001,
            "ARB": 0.001,
            "AVAX": 0.001,
            "DOGE": 0.00001,
            "LINK": 0.001,
            "MATIC": 0.0001,
            "XRP": 0.0001,
            "BNB": 0.01,
            "MKR": 0.1  # MKR uses 0.1 increments
        }
        self.tick_sizes.update(default_tick_sizes)
        logger.info(f"Using default tick sizes: {self.tick_sizes}")
    
    def round_to_tick_size(self, price, symbol):
        """Round price to the appropriate tick size for the symbol"""
        tick_size = self.tick_sizes.get(symbol, 0.001)  # Default to 0.001 if unknown
        
        # Round to nearest tick size
        rounded_price = round(price / tick_size) * tick_size
        
        # Format to avoid floating point errors
        decimals = max(0, int(-math.log10(tick_size))) if tick_size > 0 else 0
        rounded_price = round(rounded_price, decimals)
        
        # Add logging to see exact rounded price
        logger.info(f"Final rounded price for {symbol}: ${rounded_price} (tick size: {tick_size})")
        
        return rounded_price
    
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
        
        # Track symbols with active positions and open orders
        active_symbols = set()
        open_order_symbols = set()
        
        try:
            # Get positions directly from the exchange first
            try:
                state = self.info.user_state(self.address)
                if isinstance(state, dict) and "assetPositions" in state:
                    for pos in state["assetPositions"]:
                        if isinstance(pos, dict) and "coin" in pos:
                            # Check if position size is non-zero
                            if abs(float(pos.get("szi", 0))) > 0:
                                active_symbols.add(pos["coin"])
                                # Log each active position found on the exchange
                                logger.info(f"Found active position on exchange: {pos['coin']} with size {pos.get('szi', 0)}")
            except Exception as e:
                logger.error(f"Error getting positions from exchange: {e}")
            
            # Add positions from our internal tracking
            for pos_id, pos in self.open_positions.items():
                if isinstance(pos, dict) and "symbol" in pos:
                    symbol = pos["symbol"]
                    active_symbols.add(symbol)
                    # Log each position we're tracking internally
                    logger.info(f"Tracking internal position: {symbol}")
            
            # Add symbols with open orders
            for symbol, order_info in self.open_orders.items():
                open_order_symbols.add(symbol)
                logger.info(f"Tracking open order for: {symbol}")
            
            logger.info(f"Symbols with existing positions: {active_symbols}")
            logger.info(f"Symbols with open orders: {open_order_symbols}")
            
            # Create a combined set of all symbols that are active in any way
            all_active_symbols = active_symbols.union(open_order_symbols)
            logger.info(f"All active symbols (positions + orders): {all_active_symbols}")
            
            # Process up to 3 signals at once to avoid overloading
            processed_count = 0
            max_signals_per_run = 3
            
            # Now process each signal file
            for signal_file in signal_files:
                # Skip files we've already processed
                if signal_file.name in self.processed_signals:
                    continue
                
                # Limit the number of signals processed per run
                if processed_count >= max_signals_per_run:
                    logger.info(f"Reached max signals per run ({max_signals_per_run}). Will process remaining signals next cycle.")
                    break
                
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
                    
                    # **** ENHANCED CHECK: This is where we check for existing positions ****
                    # Check if we already have a position or open order for this symbol
                    if exchange_symbol in all_active_symbols:
                        # Additional detailed logging to diagnose the issue
                        is_in_active = exchange_symbol in active_symbols
                        is_in_open_orders = exchange_symbol in open_order_symbols
                        
                        logger.warning(f"Already have an active position or order for {exchange_symbol}.")
                        logger.warning(f"Details - In active positions: {is_in_active}, In open orders: {is_in_open_orders}")
                        
                        # Mark signal as processed but add a note about why it was ignored
                        signal['processed'] = True
                        signal['ignored_reason'] = f"Symbol already has an {'open position' if is_in_active else 'open order'}"
                        
                        with open(signal_file, 'w') as f:
                            json.dump(signal, f, indent=2)
                        
                        # Archive the signal file
                        target = self.archive_dir / signal_file.name
                        signal_file.rename(target)
                        self.processed_signals.add(signal_file.name)
                        continue
                    
                    # Check if we're at max positions (excluding open orders)
                    active_position_count = len(active_symbols)
                    
                    if active_position_count >= self.max_positions:
                        logger.warning(f"Reached maximum number of positions ({self.max_positions}). Skipping signal.")
                        continue
                    
                    # Check if we have an open order for this symbol - this shouldn't happen now with the enhanced check,
                    # but keeping it for safety
                    if exchange_symbol in open_order_symbols:
                        logger.info(f"Found existing open order for {exchange_symbol}, canceling before placing new order")
                        
                        # Get the open order info
                        open_order_info = self.open_orders.get(exchange_symbol)
                        if open_order_info and 'oid' in open_order_info:
                            # Cancel the existing order
                            try:
                                cancel_resp = self.exchange.cancel(open_order_info['oid'])
                                logger.info(f"Canceled order {open_order_info['oid']} for {exchange_symbol}")
                                
                                # Wait a moment for the cancellation to process
                                await asyncio.sleep(1.0)
                                
                                # Remove from tracking
                                del self.open_orders[exchange_symbol]
                            except Exception as e:
                                logger.error(f"Error canceling order {open_order_info['oid']}: {e}")
                                # Continue anyway - we'll try to place the new order
                    
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
                    
                    # Validate take profit and stop loss
                    # For LONG: TP > entry > SL
                    # For SHORT: TP < entry < SL
                    valid_tpsl = True
                    if is_long:
                        if take_profit <= entry_price:
                            logger.warning(f"Invalid TP for LONG: {take_profit} should be > {entry_price}")
                            take_profit = entry_price * 1.01  # Use 1% above entry as default
                            logger.info(f"Using default TP: {take_profit}")
                            valid_tpsl = False
                        if stop_loss >= entry_price:
                            logger.warning(f"Invalid SL for LONG: {stop_loss} should be < {entry_price}")
                            stop_loss = entry_price * 0.99  # Use 1% below entry as default
                            logger.info(f"Using default SL: {stop_loss}")
                            valid_tpsl = False
                    else:  # SHORT
                        if take_profit >= entry_price:
                            logger.warning(f"Invalid TP for SHORT: {take_profit} should be < {entry_price}")
                            take_profit = entry_price * 0.99  # Use 1% below entry as default
                            logger.info(f"Using default TP: {take_profit}")
                            valid_tpsl = False
                        if stop_loss <= entry_price:
                            logger.warning(f"Invalid SL for SHORT: {stop_loss} should be > {entry_price}")
                            stop_loss = entry_price * 1.01  # Use 1% above entry as default
                            logger.info(f"Using default SL: {stop_loss}")
                            valid_tpsl = False
                    
                    # Round values to tick size
                    entry_price = self.round_to_tick_size(entry_price, exchange_symbol)
                    take_profit = self.round_to_tick_size(take_profit, exchange_symbol)
                    stop_loss = self.round_to_tick_size(stop_loss, exchange_symbol)
                    
                    logger.info(f"Rounded prices - Entry: ${entry_price}, TP: ${take_profit}, SL: ${stop_loss}")
                    
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
                    
                    # Make sure position size is at least the minimum
                    min_size = 0.001 if exchange_symbol == "BTC" else 0.01
                    if position_size < min_size:
                        logger.warning(f"Position size {position_size} below minimum. Using {min_size} for {exchange_symbol}")
                        position_size = min_size
                    
                    logger.info(f"Using position size for {exchange_symbol}: {position_size} contracts")
                    
                    # Last safety check before executing trade - check again if the symbol has become active
                    # during our processing (rare but possible)
                    try:
                        state = self.info.user_state(self.address)
                        if isinstance(state, dict) and "assetPositions" in state:
                            for pos in state["assetPositions"]:
                                if isinstance(pos, dict) and "coin" in pos and pos["coin"] == exchange_symbol:
                                    # Check if position size is non-zero
                                    if abs(float(pos.get("szi", 0))) > 0:
                                        logger.warning(f"Last-minute check found a position for {exchange_symbol}. Skipping.")
                                        
                                        # Mark signal as processed but add a note about why it was ignored
                                        signal['processed'] = True
                                        signal['ignored_reason'] = "Symbol already has an open position (detected in last-minute check)"
                                        
                                        with open(signal_file, 'w') as f:
                                            json.dump(signal, f, indent=2)
                                        
                                        # Archive the signal file
                                        target = self.archive_dir / signal_file.name
                                        signal_file.rename(target)
                                        self.processed_signals.add(signal_file.name)
                                        continue
                    except Exception as e:
                        logger.error(f"Error in last-minute position check: {e}")
                    
                    # Add to all_active_symbols to prevent other signals from processing the same symbol in this batch
                    all_active_symbols.add(exchange_symbol)
                    
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
                    
                    if isinstance(result, dict) and result.get('status') == 'success':
                        # Mark signal as processed in the file
                        signal['processed'] = True
                        with open(signal_file, 'w') as f:
                            json.dump(signal, f, indent=2)
                        
                        # Archive the signal file
                        target = self.archive_dir / signal_file.name
                        signal_file.rename(target)
                        self.processed_signals.add(signal_file.name)
                        logger.info(f"Signal {signal_file.name} processed and archived")
                        
                        # Add to active symbols if filled, or to open orders if still open
                        if result.get('order_status') == 'filled':
                            active_symbols.add(exchange_symbol)
                        elif result.get('order_status') == 'open':
                            open_order_symbols.add(exchange_symbol)
                        
                        # Increment processed count
                        processed_count += 1
                        
                        # Update account info after executing a trade
                        await self.update_account_info()
                    elif isinstance(result, dict) and result.get('status') == 'open_order':
                        # Mark signal as being processed with an open order
                        signal['processing'] = True
                        signal['order_id'] = result.get('oid')
                        with open(signal_file, 'w') as f:
                            json.dump(signal, f, indent=2)
                        
                        logger.info(f"Signal {signal_file.name} has an open order {result.get('oid')} - keeping signal file")
                        
                        # Add to open order symbols
                        open_order_symbols.add(exchange_symbol)
                        
                        # Increment processed count
                        processed_count += 1
                    else:
                        error_reason = result.get('message', str(result)) if result else "Unknown error"
                        logger.warning(f"Failed to process signal {signal_file.name} - will retry later. Reason: {error_reason}")
                    
                except Exception as e:
                    logger.error(f"Error processing signal {signal_file}: {e}", exc_info=True)
                    logger.warning(f"Failed to process signal {signal_file.name} - will retry later. Reason: {str(e)}")
            
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
    ):
        """Execute a trade based on signal parameters"""
        try:
            # Without ticker, we'll use the entry price as our reference
            current_price = entry_price
            logger.info(f"Using signal entry price for {symbol}: ${current_price}")
            
            # Revised entry message for limit order
            logger.info(f"Executing {'LONG' if is_long else 'SHORT'} for {symbol} using LIMIT order")
            logger.info(f"Size: {size}, Entry: ${entry_price}, TP: ${take_profit}, SL: ${stop_loss}")
            
            # Validate take_profit and stop_loss values
            if is_long:
                if take_profit <= entry_price:
                    error_msg = f"Invalid take_profit for LONG position: {take_profit} <= {entry_price}"
                    logger.error(error_msg)
                    return {'status': 'error', 'message': error_msg}
                if stop_loss >= entry_price:
                    error_msg = f"Invalid stop_loss for LONG position: {stop_loss} >= {entry_price}"
                    logger.error(error_msg)
                    return {'status': 'error', 'message': error_msg}
            else:  # SHORT
                if take_profit >= entry_price:
                    error_msg = f"Invalid take_profit for SHORT position: {take_profit} >= {entry_price}"
                    logger.error(error_msg)
                    return {'status': 'error', 'message': error_msg}
                if stop_loss <= entry_price:
                    error_msg = f"Invalid stop_loss for SHORT position: {stop_loss} <= {entry_price}"
                    logger.error(error_msg)
                    return {'status': 'error', 'message': error_msg}
            
            # Place entry order
            try:
                # Revised order placement to match the working example
                logger.info(f"Placing {'LONG' if is_long else 'SHORT'} limit: size={size}, price={entry_price}")
                entry_order = self.exchange.order(
                    symbol, is_long, size, entry_price,
                    {"limit": {"tif": "Gtc"}},
                    reduce_only=False
                )
                
                logger.info(f"Entry order placed")
                
                # Process the entry order response
                if "response" in entry_order and "data" in entry_order["response"] and "statuses" in entry_order["response"]["data"]:
                    statuses = entry_order["response"]["data"]["statuses"]
                    
                    # Check for errors first
                    error_status = next((s for s in statuses if "error" in s), None)
                    if error_status:
                        error_msg = error_status.get("error", "Unknown error")
                        logger.error(f"Order error: {error_msg}")
                        logger.error(f"Full response: {json.dumps(entry_order, indent=2)}")
                        return {'status': 'error', 'message': error_msg}
                    
                    # Check for resting or filled status
                    resting_status = next((s for s in statuses if "resting" in s), None)
                    filled_status = next((s for s in statuses if "filled" in s), None)
                    
                    # Get the order ID (either from resting or filled status)
                    if resting_status:
                        entry_oid = resting_status["resting"]["oid"]
                        logger.info(f"Entry order resting: {entry_oid}, will remain active until filled or canceled")
                        
                        # Store the open order information by symbol
                        self.open_orders[symbol] = {
                            'oid': entry_oid,
                            'signal_id': signal_id,
                            'is_long': is_long,
                            'entry_price': entry_price,
                            'size': size,
                            'take_profit': take_profit,
                            'stop_loss': stop_loss,
                            'timestamp': time.time()
                        }
                        
                        # Return with open_order status
                        return {
                            'status': 'open_order', 
                            'oid': entry_oid, 
                            'symbol': symbol,
                            'message': 'Order placed and is active in the market'
                        }
                    
                    elif filled_status:
                        entry_oid = filled_status["filled"]["oid"]
                        logger.info(f"Entry order immediately filled: {entry_oid}")
                        
                        # Place stop loss order
                        if stop_loss > 0:
                            try:
                                sl_order = self.exchange.order(
                                    symbol, not is_long, size, entry_price,
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
                                    symbol, not is_long, size, entry_price,
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
                        
                        # Return success with filled status
                        return {
                            'status': 'success',
                            'order_status': 'filled',
                            'oid': entry_oid,
                            'symbol': symbol
                        }
                    
                    else:
                        logger.error("No filled or resting status found in order response")
                        # Print the full response for debugging
                        logger.error(f"Full response: {json.dumps(entry_order, indent=2)}")
                        return {'status': 'error', 'message': "No filled or resting status found in order response"}
                else:
                    logger.error("Invalid order response format")
                    logger.error(f"Full response: {json.dumps(entry_order, indent=2)}")
                    return {'status': 'error', 'message': "Invalid order response format"}
                
            except Exception as e:
                logger.error(f"Error placing entry order: {e}")
                return {'status': 'error', 'message': f"Error placing entry order: {e}"}
            
        except Exception as e:
            logger.error(f"Error executing trade: {e}", exc_info=True)
            return {'status': 'error', 'message': f"Error executing trade: {e}"}

    async def with_retries(self, fn, *args, retries=3, backoff=1, **kwargs):
        """Execute a function with retries and exponential backoff"""
        attempt = 0
        while True:
            try:
                r = fn(*args, **kwargs)
                return await r if asyncio.iscoroutine(r) else r
            except Exception as e:
                attempt += 1
                if attempt > retries:
                    logger.error(f"{fn.__name__} failed after {retries} retries: {e}")
                    raise
                wait = backoff * (2 ** (attempt - 1))
                logger.warning(f"{fn.__name__} error ({e}), retrying in {wait:.1f}s...")
                await asyncio.sleep(wait)
    
    async def wait_for_fill(self, info: Info, address: str, oid: int, max_wait_time=60):
        """
        Check if an order has been filled.
        
        This function checks the status of an order for up to max_wait_time seconds.
        Instead of raising an exception when timeout is reached, it returns the current
        status and allows the order to remain active in the market.
        
        Parameters:
        -----------
        info : Info
            The Hyperliquid info client
        address : str
            The wallet address
        oid : int
            The order ID to check
        max_wait_time : int
            Maximum seconds to actively wait before returning status (default: 60)
            
        Returns:
        --------
        dict
            A dictionary with status information including:
            - status: "filled", "canceled", or "open"
            - additional data depending on the status
        """
        start_time = time.time()
        last_log_time = start_time
        check_count = 0
        
        # Log initial check
        logger.info(f"Starting to check order {oid} status (will check for up to {max_wait_time} seconds)")
        
        while True:
            try:
                check_count += 1
                current_time = time.time()
                
                # Only log every 10 seconds to avoid flooding logs
                should_log = (current_time - last_log_time) >= 10 or check_count <= 1
                
                # Check if we've waited too long
                if current_time - start_time > max_wait_time:
                    logger.info(f"Order {oid} still open after {max_wait_time} seconds of active checking. "
                            f"Order remains active in the market.")
                    
                    # Try to get one final status update for accuracy
                    try:
                        final_resp = await self.with_retries(info.query_order_by_oid, address, oid)
                        final_status = final_resp.get("order", {}).get("status")
                        avg_px = final_resp.get("order", {}).get("avgPx")
                        side = final_resp.get("order", {}).get("side")
                        sz = final_resp.get("order", {}).get("sz")
                        
                        # If by chance the order was just filled
                        if final_status == "filled":
                            logger.info(f"Order {oid} was just filled at the final check! "
                                    f"Side: {side}, Size: {sz}, Avg Price: {avg_px}")
                            return {"status": "filled", "response": final_resp}
                        elif final_status == "canceled":
                            logger.info(f"Order {oid} was canceled at the final check!")
                            return {"status": "canceled", "oid": oid}
                        
                        # Get more detailed information about the order
                        remaining = final_resp.get("order", {}).get("remaining")
                        limit_px = final_resp.get("order", {}).get("limitPx")
                        
                        # Return detailed information about the open order
                        return {
                            "status": "open", 
                            "oid": oid,
                            "details": {
                                "remaining": remaining,
                                "limitPx": limit_px,
                                "side": side,
                                "sz": sz
                            }
                        }
                    except Exception as e:
                        logger.error(f"Error getting final status for order {oid}: {e}")
                        # If we can't get the final status, return a simpler response
                        return {"status": "open", "oid": oid}
                
                # Query order status
                resp = await self.with_retries(info.query_order_by_oid, address, oid)
                status = resp.get("order", {}).get("status")
                
                if should_log:
                    logger.info(f"Order {oid} status: {status} (check #{check_count})")
                    last_log_time = current_time
                
                if status == "filled":
                    # Get fill details
                    avg_px = resp.get("order", {}).get("avgPx")
                    side = resp.get("order", {}).get("side")
                    sz = resp.get("order", {}).get("sz")
                    
                    logger.info(f"Order {oid} has been filled! Side: {side}, Size: {sz}, Avg Price: {avg_px}")
                    return {"status": "filled", "response": resp}
                elif status == "canceled":
                    logger.info(f"Order {oid} was canceled")
                    return {"status": "canceled", "oid": oid}
                
                # Wait before checking again - use an exponential backoff approach
                # Start with short intervals, then increase as time passes
                elapsed_seconds = current_time - start_time
                if elapsed_seconds < 10:
                    # First 10 seconds: check every 1 second
                    wait_time = 1.0
                elif elapsed_seconds < 30:
                    # Next 20 seconds: check every 2 seconds 
                    wait_time = 2.0
                else:
                    # After 30 seconds: check every 5 seconds
                    wait_time = 5.0
                    
                await asyncio.sleep(wait_time)
                
            except Exception as e:
                logger.error(f"Error checking order status: {e}")
                await asyncio.sleep(2.0)
                
                # If we've been trying for too long, assume it's still open
                if time.time() - start_time > max_wait_time:
                    logger.info(f"Assuming order {oid} is still open after errors (timeout reached)")
                    return {"status": "open", "oid": oid, "error": str(e)}
        
    async def check_positions(self):
        """
        Check status of open positions and pending orders.
        
        This function performs several important maintenance tasks:
        1. Cleans up expired position tracking entries
        2. Checks for and removes duplicate position tracking entries
        3. Verifies status of open orders and updates tracking accordingly
        4. Detects closed positions and updates tracking
        5. Places take profit and stop loss orders for any filled open orders
        
        The function runs periodically in the main loop to ensure order tracking
        stays accurate and positions are properly managed.
        """
        try:
            current_time = time.time()
            logger.info("Running position and order status check")
            
            # --- STEP 1: Clean up expired position tracking ---
            positions_to_remove = []
            
            for pos_id, pos_info in self.open_positions.items():
                # Check if position is older than 24 hours
                if current_time - pos_info.get("entry_time", 0) > 86400:  # 24 hours
                    positions_to_remove.append(pos_id)
            
            # Remove expired positions
            for pos_id in positions_to_remove:
                logger.info(f"Removing expired position tracking for {pos_id}")
                self.open_positions.pop(pos_id, None)
            
            # --- STEP 2: Check for duplicate position tracking entries ---
            symbol_to_posid = {}
            duplicate_positions = []
            
            for pos_id, pos_info in self.open_positions.items():
                if isinstance(pos_info, dict) and "symbol" in pos_info:
                    symbol = pos_info["symbol"]
                    
                    if symbol in symbol_to_posid:
                        # Found a duplicate! Keep the newer one
                        existing_pos_id = symbol_to_posid[symbol]
                        existing_entry_time = self.open_positions[existing_pos_id].get("entry_time", 0)
                        current_entry_time = pos_info.get("entry_time", 0)
                        
                        if current_entry_time > existing_entry_time:
                            # Current one is newer, mark existing for removal
                            duplicate_positions.append(existing_pos_id)
                            symbol_to_posid[symbol] = pos_id
                            logger.warning(f"Found duplicate position tracking for {symbol}. Keeping newer entry ({pos_id})")
                        else:
                            # Existing one is newer, mark current for removal
                            duplicate_positions.append(pos_id)
                            logger.warning(f"Found duplicate position tracking for {symbol}. Keeping newer entry ({existing_pos_id})")
                    else:
                        symbol_to_posid[symbol] = pos_id
            
            # Remove duplicate position entries
            for pos_id in duplicate_positions:
                symbol = self.open_positions[pos_id].get("symbol", "unknown")
                logger.warning(f"Removing duplicate position tracking for {symbol} (ID: {pos_id})")
                self.open_positions.pop(pos_id, None)
            
            # --- STEP 3: Verify status of open orders and update tracking ---
            symbols_to_remove = []
            symbols_filled = []
            
            for symbol, order_info in self.open_orders.items():
                if 'oid' in order_info:
                    oid = order_info['oid']
                    logger.info(f"Checking status of open order {oid} for {symbol}")
                    
                    try:
                        resp = await self.with_retries(self.info.query_order_by_oid, self.address, oid)
                        status = resp.get("order", {}).get("status")
                        
                        if status == "filled":
                            logger.info(f"Open order {oid} for {symbol} was filled")
                            
                            # Create a position entry for the filled order
                            position_id = f"{symbol}_{oid}"
                            
                            # Prepare position info with all available details
                            position_info = {
                                "signal_id": order_info.get("signal_id"),
                                "symbol": symbol,
                                "is_long": order_info.get("is_long"),
                                "entry_price": order_info.get("entry_price"),
                                "current_size": order_info.get("size"),
                                "take_profit": order_info.get("take_profit"),
                                "stop_loss": order_info.get("stop_loss"),
                                "entry_time": current_time,
                                "order_fill_time": current_time
                            }
                            
                            # Try to get actual fill price and add to position info
                            try:
                                avg_px = float(resp.get("order", {}).get("avgPx", 0))
                                if avg_px > 0:
                                    position_info["actual_entry_price"] = avg_px
                                    logger.info(f"Order filled at price: ${avg_px}")
                            except Exception as e:
                                logger.error(f"Error getting fill price: {e}")
                            
                            # Add to our position tracking
                            self.open_positions[position_id] = position_info
                            
                            # Add to filled symbols list for TP/SL placement
                            symbols_filled.append((symbol, position_id, position_info))
                            
                            # Mark for removal from open orders
                            symbols_to_remove.append(symbol)
                        
                        elif status == "canceled":
                            logger.info(f"Open order {oid} for {symbol} was canceled")
                            symbols_to_remove.append(symbol)
                        
                        elif status == "open" or status == "resting":
                            # Order is still open
                            # Update any order information if needed
                            timestamp = order_info.get('timestamp', 0)
                            age_hours = (current_time - timestamp) / 3600
                            logger.info(f"Order {oid} for {symbol} is still open (age: {age_hours:.1f} hours)")
                            
                            # Update the timestamp to prevent unnecessary aging
                            if 'timestamp' in order_info and current_time - order_info['timestamp'] > 86400:
                                # Only update timestamp once per day to avoid unnecessary writes
                                order_info['timestamp'] = current_time
                                logger.info(f"Updated timestamp for long-running order {oid}")
                        
                        else:
                            logger.warning(f"Unknown order status for {oid}: {status}")
                            # If we don't understand the status, better to remove it to avoid issues
                            symbols_to_remove.append(symbol)
                    
                    except Exception as e:
                        logger.error(f"Error checking order {oid} for {symbol}: {e}")
                        # If we get an error, the order might be too old or invalid
                        error_str = str(e).lower()
                        if "not found" in error_str or "invalid" in error_str or "no such" in error_str:
                            logger.warning(f"Order {oid} appears to be invalid or no longer exists. Removing from tracking.")
                            symbols_to_remove.append(symbol)
                else:
                    # Missing order ID, can't check status
                    logger.warning(f"Open order for {symbol} is missing order ID. Removing from tracking.")
                    symbols_to_remove.append(symbol)
            
            # Remove filled or canceled orders from tracking
            for symbol in symbols_to_remove:
                if symbol in self.open_orders:
                    self.open_orders.pop(symbol, None)
                    logger.info(f"Removed {symbol} from open orders tracking")
            
            # --- STEP 4: Place TP/SL for filled orders ---
            for symbol, position_id, position_info in symbols_filled:
                # Place stop loss order
                if position_info.get("stop_loss", 0) > 0:
                    try:
                        is_long = position_info.get("is_long", True)
                        size = position_info.get("current_size", 0)
                        stop_loss = position_info.get("stop_loss", 0)
                        
                        # Use actual entry price if available, otherwise use planned entry price
                        entry_price = position_info.get("actual_entry_price", position_info.get("entry_price", 0))
                        
                        logger.info(f"Placing stop loss order for filled order: {symbol} "
                                f"{'LONG' if is_long else 'SHORT'} {size} @ {entry_price} -> SL: {stop_loss}")
                        
                        sl_order = self.exchange.order(
                            symbol, not is_long, size, entry_price,
                            {"trigger": {"tpsl": "sl", "triggerPx": stop_loss, "isMarket": True}},
                            reduce_only=True
                        )
                        
                        # Check and log the response
                        if "response" in sl_order and "data" in sl_order["response"]:
                            logger.info(f"Stop loss order placed for filled order {position_id}")
                        else:
                            logger.error(f"Error placing stop loss for filled order {position_id}: {sl_order}")
                    
                    except Exception as e:
                        logger.error(f"Error placing stop loss for filled order {position_id}: {e}")
                
                # Place take profit order
                if position_info.get("take_profit", 0) > 0:
                    try:
                        is_long = position_info.get("is_long", True)
                        size = position_info.get("current_size", 0)
                        take_profit = position_info.get("take_profit", 0)
                        
                        # Use actual entry price if available, otherwise use planned entry price
                        entry_price = position_info.get("actual_entry_price", position_info.get("entry_price", 0))
                        
                        logger.info(f"Placing take profit order for filled order: {symbol} "
                                f"{'LONG' if is_long else 'SHORT'} {size} @ {entry_price} -> TP: {take_profit}")
                        
                        tp_order = self.exchange.order(
                            symbol, not is_long, size, entry_price,
                            {"trigger": {"tpsl": "tp", "triggerPx": take_profit, "isMarket": True}},
                            reduce_only=True
                        )
                        
                        # Check and log the response
                        if "response" in tp_order and "data" in tp_order["response"]:
                            logger.info(f"Take profit order placed for filled order {position_id}")
                        else:
                            logger.error(f"Error placing take profit for filled order {position_id}: {tp_order}")
                    
                    except Exception as e:
                        logger.error(f"Error placing take profit for filled order {position_id}: {e}")
            
            # --- STEP 5: Check if any open positions were closed on the exchange ---
            if self.open_positions:
                try:
                    state = self.info.user_state(self.address)
                    
                    if isinstance(state, dict) and "assetPositions" in state:
                        # Get current positions from the exchange
                        exchange_positions = {}
                        for pos in state["assetPositions"]:
                            if isinstance(pos, dict) and "coin" in pos:
                                symbol = pos["coin"]
                                size = float(pos.get("szi", 0))
                                if abs(size) > 0:
                                    exchange_positions[symbol] = {
                                        "size": size,
                                        "entry_price": float(pos.get("entryPx", 0)),
                                        "unrealized_pnl": float(pos.get("unrealizedPnl", 0)),
                                        "mark_price": float(pos.get("markPx", 0))
                                    }
                        
                        # Check if any of our tracked positions were closed
                        positions_closed = []
                        for pos_id, pos_info in self.open_positions.items():
                            symbol = pos_info.get("symbol")
                            if symbol and symbol not in exchange_positions:
                                positions_closed.append(pos_id)
                        
                        # Remove closed positions from our tracking
                        for pos_id in positions_closed:
                            pos_info = self.open_positions.get(pos_id)
                            if pos_info:
                                symbol = pos_info.get("symbol", "unknown")
                                entry_time = pos_info.get("entry_time", 0)
                                duration_hours = (current_time - entry_time) / 3600 if entry_time > 0 else 0
                                
                                logger.info(f"Position {pos_id} for {symbol} was closed on the exchange "
                                        f"(duration: {duration_hours:.1f} hours)")
                                
                                self.open_positions.pop(pos_id, None)
                        
                        # If positions were closed, update account info
                        if positions_closed:
                            await self.update_account_info()
                        
                        # Also update any position information that might have changed
                        positions_updated = 0
                        for pos_id, pos_info in self.open_positions.items():
                            symbol = pos_info.get("symbol")
                            if symbol and symbol in exchange_positions:
                                # Update position information with latest from exchange
                                exchange_pos = exchange_positions[symbol]
                                pos_info["current_size"] = abs(exchange_pos["size"])
                                pos_info["current_entry_price"] = exchange_pos["entry_price"]
                                pos_info["unrealized_pnl"] = exchange_pos["unrealized_pnl"]
                                pos_info["mark_price"] = exchange_pos["mark_price"]
                                pos_info["last_updated"] = current_time
                                positions_updated += 1
                        
                        if positions_updated > 0:
                            logger.info(f"Updated information for {positions_updated} active positions")
                    
                except Exception as e:
                    logger.error(f"Error checking positions on exchange: {e}", exc_info=True)
            
            # Log summary of current state
            num_open_positions = len(self.open_positions)
            num_open_orders = len(self.open_orders)
            logger.info(f"Position check complete. Current state: {num_open_positions} open positions, "
                    f"{num_open_orders} open orders")
            
        except Exception as e:
            logger.error(f"Error in check_positions: {e}", exc_info=True)

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