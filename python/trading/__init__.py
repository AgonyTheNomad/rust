#!/usr/bin/env python3
"""
Main trader class for the Hyperliquid trading bot.
"""

import os
import json
import time
import asyncio
import logging
from pathlib import Path
from datetime import datetime, timezone

from trading import price_utils
from trading.position_manager import PositionManager
from trading.signal_processor import SignalProcessor
from trading.order_executor import OrderExecutor
from trading.command_handler import CommandHandler

logger = logging.getLogger("hyperliquid_trader")

class HyperliquidTrader:
    """
    Main trader class that orchestrates the trading process.
    This class coordinates all the components of the trading system.
    """
    
    def __init__(self, config_path: str, signals_dir: str, archive_dir: str, commands_dir: str):
        """
        Initialize the Hyperliquid trader.
        
        Parameters:
        -----------
        config_path : str
            Path to the configuration file
        signals_dir : str
            Path to the directory containing signal files
        archive_dir : str
            Path to the directory for archiving processed signals
        commands_dir : str
            Path to the directory containing command files
        """
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
        self.is_paused = False
        self.tick_sizes = {}
        
        # Configuration parameters
        self.max_signal_age = self.config.get('max_signal_age_minutes', 5)
        self.max_positions = self.config.get('max_positions', 5)
        self.symbol_mapping = self.config.get('symbol_mapping', {})
        
        # Setup Hyperliquid client
        from utils import setup
        self.address, self.info, self.exchange = setup(skip_ws=False)
        
        # Initialize components
        self.position_manager = PositionManager(self.info, self.exchange, self.address, self.max_positions)
        self.order_executor = OrderExecutor(self.exchange, self.info, self.address)
        self.signal_processor = SignalProcessor(
            self.signals_dir, 
            self.archive_dir, 
            self.symbol_mapping, 
            self.max_signal_age,
            self.max_positions,
            self.position_manager,
            self.order_executor,
            self.config
        )
        self.command_handler = CommandHandler(
            self.commands_dir, 
            self.archive_dir, 
            self,
            self.config_path
        )
        
        logger.info(f"Hyperliquid trader initialized with config: {config_path}")
        logger.info(f"Using signals directory: {signals_dir}")
        logger.info(f"Using {'TESTNET' if self.config.get('use_testnet') else 'MAINNET'}")
    
    async def fetch_asset_metadata(self):
        """
        Fetch and store metadata for all assets including tick sizes.
        """
        try:
            # Get metadata from the API
            meta = self.info.meta()
            universe = meta.get("universe", [])
            
            # Extract tick sizes from the metadata
            self.tick_sizes = price_utils.extract_tick_sizes_from_metadata(universe)
            
            if not self.tick_sizes:
                # If we couldn't extract tick sizes, use defaults
                self.tick_sizes = price_utils.get_default_tick_sizes()
            
            # Apply critical overrides
            self.tick_sizes = price_utils.apply_critical_overrides(self.tick_sizes)
            
            logger.info(f"Loaded tick sizes for {len(self.tick_sizes)} symbols:")
            for symbol, tick in sorted(self.tick_sizes.items()):
                logger.info(f"  {symbol}: {tick}")
            
            # Share tick sizes with components that need them
            self.signal_processor.set_tick_sizes(self.tick_sizes)
            self.order_executor.set_tick_sizes(self.tick_sizes)
            
        except Exception as e:
            logger.error(f"Error fetching asset metadata: {e}", exc_info=True)
            # Use defaults if API fetch failed
            self.tick_sizes = price_utils.get_default_tick_sizes()
            self.tick_sizes = price_utils.apply_critical_overrides(self.tick_sizes)
            
            # Share tick sizes with components that need them
            self.signal_processor.set_tick_sizes(self.tick_sizes)
            self.order_executor.set_tick_sizes(self.tick_sizes)
    
    async def update_account_info(self):
        """
        Update account information and write to a file for Rust to read.
        
        Returns:
        --------
        dict or None
            Account information dictionary if successful, None otherwise
        """
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
    
    async def start(self):
        """
        Start the trading loop.
        This is the main entry point for the trader.
        """
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
                await self.command_handler.check_commands()
                
                # Update account info every 60 seconds
                current_time = time.time()
                if current_time - last_account_update > 60:
                    await self.update_account_info()
                    last_account_update = current_time
                
                if not self.is_paused:
                    # Find and process new signals
                    await self.signal_processor.process_signals()
                    
                    # Check status of open positions
                    positions_updated = await self.position_manager.check_positions()
                    
                    # If positions were updated, update account info
                    if positions_updated:
                        await self.update_account_info()
                
                # Sleep for a bit
                await asyncio.sleep(1.0)
                
            except Exception as e:
                logger.error(f"Error in trading loop: {e}", exc_info=True)
                await asyncio.sleep(5.0)
    
    def set_paused(self, paused):
        """
        Set the paused state of the trader.
        
        Parameters:
        -----------
        paused : bool
            Whether to pause trading
        """
        self.is_paused = paused
        logger.info(f"Trading {'paused' if paused else 'resumed'}")
    
    def update_config(self, key, value):
        """
        Update a configuration parameter.
        
        Parameters:
        -----------
        key : str
            Configuration key to update
        value : any
            New value for the configuration key
        
        Returns:
        --------
        bool
            True if the configuration was updated, False otherwise
        """
        try:
            # Convert value to appropriate type
            if isinstance(self.config.get(key), bool):
                value = value.lower() == 'true' if isinstance(value, str) else bool(value)
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
            
            # Update component configurations if needed
            if key == 'max_positions':
                self.max_positions = value
                self.position_manager.max_positions = value
            elif key == 'max_signal_age_minutes':
                self.max_signal_age = value
                self.signal_processor.max_signal_age = value
            elif key == 'symbol_mapping':
                self.symbol_mapping = value
                self.signal_processor.symbol_mapping = value
            
            return True
            
        except Exception as e:
            logger.error(f"Error updating config: {e}")
            return False