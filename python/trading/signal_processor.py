#!/usr/bin/env python3
"""
Signal Processor for the Hyperliquid trading bot.
Handles finding and processing signal files for trading.
"""

import json
import logging
import asyncio
from pathlib import Path
from datetime import datetime, timezone
from typing import Dict, Any, Set, List

from trading import price_utils

logger = logging.getLogger("hyperliquid_trader")

class SignalProcessor:
    """
    Processes trading signals from signal files.
    """
    
    def __init__(
        self, 
        signals_dir, 
        archive_dir, 
        symbol_mapping, 
        max_signal_age, 
        max_positions,
        position_manager,
        order_executor,
        config
    ):
        """
        Initialize the signal processor.
        
        Parameters:
        -----------
        signals_dir : str or Path
            Directory containing signal files
        archive_dir : str or Path
            Directory for archiving processed signals
        symbol_mapping : dict
            Dictionary mapping external symbols to exchange symbols
        max_signal_age : int
            Maximum age of signals in minutes
        max_positions : int
            Maximum number of positions allowed
        position_manager : PositionManager
            Position manager instance
        order_executor : OrderExecutor
            Order executor instance
        config : dict
            Configuration dictionary
        """
        self.signals_dir = Path(signals_dir)
        self.archive_dir = Path(archive_dir)
        self.symbol_mapping = symbol_mapping
        self.max_signal_age = max_signal_age
        self.max_positions = max_positions
        self.position_manager = position_manager
        self.order_executor = order_executor
        self.config = config
        self.processed_signals = set()
        self.tick_sizes = {}
        
        # Create directories if they don't exist
        self.signals_dir.mkdir(exist_ok=True)
        self.archive_dir.mkdir(exist_ok=True)
        
        logger.info(f"Signal processor initialized with directory: {signals_dir}")
    
    def set_tick_sizes(self, tick_sizes: Dict[str, float]):
        """
        Set tick sizes for all symbols.
        
        Parameters:
        -----------
        tick_sizes : Dict[str, float]
            Dictionary mapping symbol to tick size
        """
        self.tick_sizes = tick_sizes
        logger.info(f"Signal processor updated with {len(tick_sizes)} tick sizes")
    
    async def process_signals(self):
        """
        Find and process new signal files.
        """
        signal_files = list(self.signals_dir.glob('*.json'))
        
        if not signal_files:
            return
        
        # Sort by creation time
        signal_files.sort(key=lambda p: p.stat().st_mtime)
        
        # Get active symbols first
        active_symbols, open_order_symbols, all_active_symbols = await self.position_manager.get_active_symbols()
        
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
                
                # Process the signal
                result = await self.process_signal(signal, signal_file, all_active_symbols, active_symbols, open_order_symbols)
                
                if result:
                    processed_count += 1
                
            except Exception as e:
                logger.error(f"Error processing signal {signal_file}: {e}")
    
    async def process_signal(self, signal, signal_file, all_active_symbols, active_symbols, open_order_symbols):
        """
        Process a single trading signal.
        
        Parameters:
        -----------
        signal : dict
            Signal data
        signal_file : Path
            Path to the signal file
        all_active_symbols : set
            Set of all active symbols (positions + orders)
        active_symbols : set
            Set of symbols with active positions
        open_order_symbols : set
            Set of symbols with open orders
            
        Returns:
        --------
        bool
            True if the signal was processed, False otherwise
        """
        try:
            # Fix for datetime comparison issue
            signal_time = datetime.fromisoformat(signal['timestamp'].replace('Z', '+00:00'))
            now_utc = datetime.now(timezone.utc)
            age_minutes = (now_utc - signal_time).total_seconds() / 60
            
            if age_minutes > self.max_signal_age:
                logger.warning(f"Signal {signal_file.name} is too old ({age_minutes:.1f} min). Archiving.")
                target = self.archive_dir / signal_file.name
                signal_file.rename(target)
                self.processed_signals.add(signal_file.name)
                return False
            
            # Process the signal
            logger.info(f"Processing signal: {signal_file.name}")
            
            # Map the symbol if needed
            symbol = signal['symbol']
            exchange_symbol = self.symbol_mapping.get(symbol, symbol)
            
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
                return False
            
            # Check if we're at max positions (excluding open orders)
            active_position_count = len(active_symbols)
            
            if active_position_count >= self.max_positions:
                logger.warning(f"Reached maximum number of positions ({self.max_positions}). Skipping signal.")
                return False
            
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
            if self.tick_sizes:
                entry_price = price_utils.round_to_tick_size(entry_price, exchange_symbol, self.tick_sizes)
                take_profit = price_utils.round_to_tick_size(take_profit, exchange_symbol, self.tick_sizes)
                stop_loss = price_utils.round_to_tick_size(stop_loss, exchange_symbol, self.tick_sizes)
                
                logger.info(f"Rounded prices - Entry: ${entry_price}, TP: ${take_profit}, SL: ${stop_loss}")
            
            # Use signal strength or risk_per_trade from config
            strength = float(signal.get('strength', 0.8))
            risk_per_trade = self.config.get('risk_per_trade', 0.01)
            effective_risk = risk_per_trade * strength
            
            # Get position size directly from the signal if available
            position_size = float(signal.get('size', 0))
            
            # Calculate position size if not provided in the signal
            if position_size <= 0:
                position_size = await self.calculate_position_size(exchange_symbol, entry_price, stop_loss, effective_risk)
            
            # Make sure position size is at least the minimum
            min_size = 0.001 if exchange_symbol == "BTC" else 0.01
            if position_size < min_size:
                logger.warning(f"Position size {position_size} below minimum. Using {min_size} for {exchange_symbol}")
                position_size = min_size
            
            logger.info(f"Using position size for {exchange_symbol}: {position_size} contracts")
            
            # Execute the trade
            result = await self.order_executor.execute_trade(
                signal_id=signal.get('id', 'unknown'),
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
                
                return True
                
            if isinstance(result, dict) and result.get('status') == 'open_order':
                # Mark signal as being processed with an open order
                signal['processing'] = True
                signal['order_id'] = result.get('oid')
                with open(signal_file, 'w') as f:
                    json.dump(signal, f, indent=2)
                
                # Move signal file to open directory
                target = self.open_dir / signal_file.name
                signal_file.rename(target)
                logger.info(f"Signal {signal_file.name} has an open order {result.get('oid')} - moved to open directory")
                
                # Add to position manager's tracked open orders
                self.position_manager.open_orders[exchange_symbol] = result
                
                return True
                
            else:
                error_reason = result.get('message', str(result)) if result else "Unknown error"
                logger.warning(f"Failed to process signal {signal_file.name} - will retry later. Reason: {error_reason}")
                return False
                
        except Exception as e:
            logger.error(f"Error processing signal: {e}")
            return False
    
    async def calculate_position_size(self, symbol, entry_price, stop_loss, risk_factor):
        """
        Calculate position size based on risk management rules.
        
        Parameters:
        -----------
        symbol : str
            Trading symbol
        entry_price : float
            Entry price
        stop_loss : float
            Stop loss price
        risk_factor : float
            Risk factor (percentage of account to risk)
            
        Returns:
        --------
        float
            Position size
        """
        try:
            # Get user state to calculate account value
            state = self.position_manager.info.user_state(self.position_manager.address)
            
            account_value = 0.0
            if isinstance(state, dict) and "crossMarginSummary" in state:
                cross_margin_summary = state["crossMarginSummary"]
                account_value = float(cross_margin_summary.get("accountValue", 0))
            
            if account_value <= 0:
                logger.warning("Account value is zero or negative. Using minimum position size.")
                return 0.01  # Use minimum size
            
            # Calculate risk amount
            risk_amount = account_value * risk_factor
            
            # Calculate risk per contract
            risk_per_contract = abs(entry_price - stop_loss)
            if risk_per_contract <= 0:
                logger.warning(f"Invalid risk per contract: {risk_per_contract}. Using default.")
                risk_per_contract = entry_price * 0.01  # Use 1% of entry price
            
            # Calculate position size in contracts
            position_size = risk_amount / risk_per_contract
            
            # Apply position limits
            max_position_size = self.config.get('max_position_size', 1.0)
            position_size = min(position_size, max_position_size)
            
            # Round to appropriate precision based on symbol
            if symbol == "BTC":
                position_size = round(position_size, 3)
            elif symbol in ["ETH", "SOL"]:
                position_size = round(position_size, 2)
            else:
                position_size = round(position_size, 1)
            
            return position_size
            
        except Exception as e:
            logger.error(f"Error calculating position size: {e}")
            return 0.01  # Use minimum size on error