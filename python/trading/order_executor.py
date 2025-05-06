#!/usr/bin/env python3
"""
Order Executor for the Hyperliquid trading bot.
Handles order execution and management.
"""

import time
import logging
import asyncio
from typing import Dict, Any, Optional

from trading import price_utils

logger = logging.getLogger("hyperliquid_trader")

class OrderExecutor:
    """
    Handles the execution and tracking of orders on the Hyperliquid exchange.
    """
    
    def __init__(self, exchange, info, address):
        """
        Initialize the order executor.
        
        Parameters:
        -----------
        exchange : hyperliquid.exchange.Exchange
            The Hyperliquid exchange client for placing orders
        info : hyperliquid.info.Info
            The Hyperliquid info client for checking orders
        address : str
            The wallet address
        """
        self.exchange = exchange
        self.info = info
        self.address = address
        self.tick_sizes = {}
    
    def set_tick_sizes(self, tick_sizes: Dict[str, float]):
        """
        Set tick sizes for all symbols.
        
        Parameters:
        -----------
        tick_sizes : Dict[str, float]
            Dictionary mapping symbol to tick size
        """
        self.tick_sizes = tick_sizes
        logger.info(f"Order executor updated with {len(tick_sizes)} tick sizes")
    
    def round_to_tick_size(self, price: float, symbol: str) -> float:
        """
        Round price to the appropriate tick size for the symbol.
        
        Parameters:
        -----------
        price : float
            The price to round
        symbol : str
            The symbol/asset to round for
        
        Returns:
        --------
        float
            Rounded price
        """
        return price_utils.round_to_tick_size(price, symbol, self.tick_sizes)
    
    async def execute_trade(
        self,
        signal_id: str,
        symbol: str,
        is_long: bool,
        entry_price: float,
        size: float,
        take_profit: float,
        stop_loss: float
    ) -> Dict[str, Any]:
        """
        Execute a trade based on signal parameters.
        
        Parameters:
        -----------
        signal_id : str
            ID of the signal
        symbol : str
            Trading symbol
        is_long : bool
            Whether this is a long position
        entry_price : float
            Entry price for the limit order
        size : float
            Position size
        take_profit : float
            Take profit price
        stop_loss : float
            Stop loss price
        
        Returns:
        --------
        Dict[str, Any]
            Result of the order execution with status and details
        """
        try:
            # Use the entry price as our reference
            current_price = entry_price
            logger.info(f"Using signal entry price for {symbol}: ${current_price}")
            
            # Log entry details
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
                        logger.error(f"Full response: {entry_order}")
                        return {'status': 'error', 'message': error_msg}
                    
                    # Check for resting or filled status
                    resting_status = next((s for s in statuses if "resting" in s), None)
                    filled_status = next((s for s in statuses if "filled" in s), None)
                    
                    # Get the order ID (either from resting or filled status)
                    if resting_status:
                        entry_oid = resting_status["resting"]["oid"]
                        logger.info(f"Entry order resting: {entry_oid}, will remain active until filled or canceled")
                        
                        # Return with open_order status
                        return {
                            'status': 'open_order', 
                            'oid': entry_oid, 
                            'symbol': symbol,
                            'signal_id': signal_id,
                            'is_long': is_long,
                            'entry_price': entry_price,
                            'size': size,
                            'take_profit': take_profit,
                            'stop_loss': stop_loss,
                            'timestamp': time.time(),
                            'message': 'Order placed and is active in the market'
                        }
                    
                    elif filled_status:
                        entry_oid = filled_status["filled"]["oid"]
                        logger.info(f"Entry order immediately filled: {entry_oid}")
                        
                        # Place stop loss order
                        sl_result = await self.place_stop_loss(
                            symbol=symbol,
                            is_long=is_long,
                            size=size,
                            entry_price=entry_price,
                            stop_loss=stop_loss
                        )
                        
                        # Place take profit order
                        tp_result = await self.place_take_profit(
                            symbol=symbol,
                            is_long=is_long,
                            size=size,
                            entry_price=entry_price,
                            take_profit=take_profit
                        )
                        
                        # Return success with filled status
                        return {
                            'status': 'success',
                            'order_status': 'filled',
                            'oid': entry_oid,
                            'symbol': symbol,
                            'signal_id': signal_id,
                            'is_long': is_long,
                            'entry_price': entry_price,
                            'size': size,
                            'take_profit': take_profit,
                            'stop_loss': stop_loss,
                            'tp_result': tp_result,
                            'sl_result': sl_result,
                            'entry_time': time.time()
                        }
                    
                    else:
                        logger.error("No filled or resting status found in order response")
                        logger.error(f"Full response: {entry_order}")
                        return {'status': 'error', 'message': "No filled or resting status found in order response"}
                else:
                    logger.error("Invalid order response format")
                    logger.error(f"Full response: {entry_order}")
                    return {'status': 'error', 'message': "Invalid order response format"}
                
            except Exception as e:
                logger.error(f"Error placing entry order: {e}")
                return {'status': 'error', 'message': f"Error placing entry order: {e}"}
            
        except Exception as e:
            logger.error(f"Error executing trade: {e}")
            return {'status': 'error', 'message': f"Error executing trade: {e}"}
    
    async def place_stop_loss(self, symbol, is_long, size, entry_price, stop_loss):
        """
        Place a stop loss order.
        
        Parameters:
        -----------
        symbol : str
            Trading symbol
        is_long : bool
            Whether this is a long position
        size : float
            Position size
        entry_price : float
            Entry price
        stop_loss : float
            Stop loss price
        
        Returns:
        --------
        dict
            Result of the stop loss order
        """
        if stop_loss <= 0:
            return {"status": "skipped", "reason": "Stop loss not provided"}
        
        try:
            logger.info(f"Placing stop loss order for {symbol}: "
                    f"{'LONG' if is_long else 'SHORT'} {size} @ {entry_price} -> SL: {stop_loss}")
            
            sl_order = self.exchange.order(
                symbol, not is_long, size, entry_price,
                {"trigger": {"tpsl": "sl", "triggerPx": stop_loss, "isMarket": True}},
                reduce_only=True
            )
            
            # Check and log the response
            if "response" in sl_order and "data" in sl_order["response"]:
                logger.info(f"Stop loss order placed for {symbol}")
                return {"status": "success", "response": sl_order}
            else:
                logger.error(f"Error placing stop loss for {symbol}: {sl_order}")
                return {"status": "error", "response": sl_order}
            
        except Exception as e:
            logger.error(f"Error placing stop loss for {symbol}: {e}")
            return {"status": "error", "message": str(e)}
    
    async def place_take_profit(self, symbol, is_long, size, entry_price, take_profit):
        """
        Place a take profit order.
        
        Parameters:
        -----------
        symbol : str
            Trading symbol
        is_long : bool
            Whether this is a long position
        size : float
            Position size
        entry_price : float
            Entry price
        take_profit : float
            Take profit price
        
        Returns:
        --------
        dict
            Result of the take profit order
        """
        if take_profit <= 0:
            return {"status": "skipped", "reason": "Take profit not provided"}
        
        try:
            logger.info(f"Placing take profit order for {symbol}: "
                    f"{'LONG' if is_long else 'SHORT'} {size} @ {entry_price} -> TP: {take_profit}")
            
            tp_order = self.exchange.order(
                symbol, not is_long, size, entry_price,
                {"trigger": {"tpsl": "tp", "triggerPx": take_profit, "isMarket": True}},
                reduce_only=True
            )
            
            # Check and log the response
            if "response" in tp_order and "data" in tp_order["response"]:
                logger.info(f"Take profit order placed for {symbol}")
                return {"status": "success", "response": tp_order}
            else:
                logger.error(f"Error placing take profit for {symbol}: {tp_order}")
                return {"status": "error", "response": tp_order}
            
        except Exception as e:
            logger.error(f"Error placing take profit for {symbol}: {e}")
            return {"status": "error", "message": str(e)}
    
    async def cancel_order(self, oid):
        """
        Cancel an order.
        
        Parameters:
        -----------
        oid : int
            Order ID to cancel
        
        Returns:
        --------
        dict
            Result of the cancel operation
        """
        try:
            logger.info(f"Canceling order: {oid}")
            cancel_resp = self.exchange.cancel(oid)
            logger.info(f"Canceled order {oid}")
            return {"status": "success", "response": cancel_resp}
        except Exception as e:
            logger.error(f"Error canceling order {oid}: {e}")
            return {"status": "error", "message": str(e)}
    
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