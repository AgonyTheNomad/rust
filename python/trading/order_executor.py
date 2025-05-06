#!/usr/bin/env python3
"""
Position Manager for the Hyperliquid trading bot.
Handles tracking and management of positions and orders.
"""

import time
import logging
import asyncio
from typing import Dict, Set, Tuple, List, Any, Optional

logger = logging.getLogger("hyperliquid_trader")

class PositionManager:
    """
    Manages tracking and updating of positions and open orders.
    """
    
    def __init__(self, info, exchange, address, max_positions=5):
        """
        Initialize the position manager.
        
        Parameters:
        -----------
        info : hyperliquid.info.Info
            The Hyperliquid info client
        exchange : hyperliquid.exchange.Exchange
            The Hyperliquid exchange client
        address : str
            The wallet address
        max_positions : int
            Maximum number of allowed positions
        """
        self.info = info
        self.exchange = exchange
        self.address = address
        self.max_positions = max_positions
        
        # Trading state
        self.open_positions = {}
        self.open_orders = {}
        
    async def get_active_symbols(self) -> Tuple[Set[str], Set[str], Set[str]]:
        """
        Get sets of symbols with active positions and open orders.
        
        Returns:
        --------
        Tuple[Set[str], Set[str], Set[str]]
            Tuple containing:
            - Set of symbols with active positions
            - Set of symbols with open orders
            - Combined set of all active symbols
        """
        active_symbols = set()
        open_order_symbols = set()
        
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
        
        # Create a combined set of all symbols that are active in any way
        all_active_symbols = active_symbols.union(open_order_symbols)
        
        logger.info(f"Symbols with existing positions: {active_symbols}")
        logger.info(f"Symbols with open orders: {open_order_symbols}")
        logger.info(f"All active symbols (positions + orders): {all_active_symbols}")
        
        return active_symbols, open_order_symbols, all_active_symbols
    
    async def check_positions(self):
        """
        Check status of open positions and pending orders.
        
        This function performs several important maintenance tasks:
        1. Cleans up expired position tracking entries
        2. Checks for and removes duplicate position tracking entries
        3. Verifies status of open orders and updates tracking accordingly
        4. Detects closed positions and updates tracking
        5. Places take profit and stop loss orders for any filled open orders
        
        Returns:
        --------
        bool
            True if any positions or orders were updated, False otherwise
        """
        try:
            current_time = time.time()
            logger.info("Running position and order status check")
            updated = False
            
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
                updated = True
            
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
                updated = True
            
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
                            updated = True
                        
                        elif status == "canceled":
                            logger.info(f"Open order {oid} for {symbol} was canceled")
                            symbols_to_remove.append(symbol)
                            updated = True
                        
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
                            updated = True
                    
                    except Exception as e:
                        logger.error(f"Error checking order {oid} for {symbol}: {e}")
                        # If we get an error, the order might be too old or invalid
                        error_str = str(e).lower()
                        if "not found" in error_str or "invalid" in error_str or "no such" in error_str:
                            logger.warning(f"Order {oid} appears to be invalid or no longer exists. Removing from tracking.")
                            symbols_to_remove.append(symbol)
                            updated = True
                else:
                    # Missing order ID, can't check status
                    logger.warning(f"Open order for {symbol} is missing order ID. Removing from tracking.")
                    symbols_to_remove.append(symbol)
                    updated = True
            
            # Remove filled or canceled orders from tracking
            for symbol in symbols_to_remove:
                if symbol in self.open_orders:
                    self.open_orders.pop(symbol, None)
                    logger.info(f"Removed {symbol} from open orders tracking")
            
            # --- STEP 4: Place TP/SL for filled orders ---
            for symbol, position_id, position_info in symbols_filled:
                await self.place_tpsl_orders(symbol, position_id, position_info)
            
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
                                updated = True
                        
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
                            updated = True
                    
                except Exception as e:
                    logger.error(f"Error checking positions on exchange: {e}", exc_info=True)
            
            # Log summary of current state
            num_open_positions = len(self.open_positions)
            num_open_orders = len(self.open_orders)
            logger.info(f"Position check complete. Current state: {num_open_positions} open positions, "
                    f"{num_open_orders} open orders")
            
            return updated
            
        except Exception as e:
            logger.error(f"Error in check_positions: {e}", exc_info=True)
            return False
    
    async def place_tpsl_orders(self, symbol, position_id, position_info):
        """
        Place take profit and stop loss orders for a position.
        
        Parameters:
        -----------
        symbol : str
            The trading symbol
        position_id : str
            The position identifier
        position_info : dict
            Dictionary with position information
        """
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
    
    async def with_retries(self, fn, *args, retries=3, backoff=1, **kwargs):
        """
        Execute a function with retries and exponential backoff.
        
        Parameters:
        -----------
        fn : function
            The function to execute
        *args : tuple
            Arguments to pass to the function
        retries : int
            Number of retries (default: 3)
        backoff : float
            Initial backoff time in seconds (default: 1)
        **kwargs : dict
            Keyword arguments to pass to the function
        
        Returns:
        --------
        any
            The return value of the function
        
        Raises:
        -------
        Exception
            If all retries fail
        """
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
    
    async def wait_for_fill(self, oid: int, max_wait_time=60):
        """
        Check if an order has been filled.
        
        This function checks the status of an order for up to max_wait_time seconds.
        Instead of raising an exception when timeout is reached, it returns the current
        status and allows the order to remain active in the market.
        
        Parameters:
        -----------
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
                        final_resp = await self.with_retries(self.info.query_order_by_oid, self.address, oid)
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
                resp = await self.with_retries(self.info.query_order_by_oid, self.address, oid)
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