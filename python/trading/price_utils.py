#!/usr/bin/env python3
"""
Price utility functions for the Hyperliquid trading bot.
Contains functions for formatting and calculating prices.
"""

import logging
import math
from typing import Dict, Any, List, Optional

logger = logging.getLogger("hyperliquid_trader")

def extract_tick_sizes_from_metadata(universe: List[dict]) -> Dict[str, float]:
    """
    Extract tick sizes from universe metadata.
    
    Parameters:
    -----------
    universe : List[dict]
        List of asset metadata from Hyperliquid API
        
    Returns:
    --------
    Dict[str, float]
        Dictionary mapping asset symbol to tick size
    """
    try:
        tick_sizes = {}
        
        for asset in universe:
            if isinstance(asset, dict) and "name" in asset:
                symbol = asset["name"]
                # For perpetuals, tick size is determined by the smallest price increment
                # Most assets use 2 decimal places for price (0.01) by default
                # A few assets use different price decimals
                
                # Default tick size is 0.01 (2 decimal places)
                tick_size = 0.01  
                
                # Handle special cases based on asset name or category
                if symbol in ["BTC"]:
                    tick_size = 0.1  # $0.10 for BTC
                elif symbol in ["ETH", "MKR", "YFI"]:
                    tick_size = 0.01  # $0.01 for mid-tier assets
                elif symbol in ["SOL", "AVAX", "LINK", "UNI", "AAVE"]:
                    tick_size = 0.001  # $0.001 for lower-tier assets
                elif symbol in ["DOGE", "SHIB", "PEPE"]:
                    tick_size = 0.00001  # $0.00001 for meme coins
                
                tick_sizes[symbol] = tick_size
                
        logger.info(f"Extracted tick sizes for {len(tick_sizes)} assets from metadata")
        return tick_sizes
    
    except Exception as e:
        logger.error(f"Error extracting tick sizes: {e}")
        return {}

def get_default_tick_sizes() -> Dict[str, float]:
    """
    Get default tick sizes for common assets.
    This is used as a fallback when API data isn't available.
    
    Returns:
    --------
    Dict[str, float]
        Dictionary mapping asset symbol to tick size
    """
    # Default tick sizes for common assets
    # These are approximate and should be updated with accurate values
    return {
        "BTC": 0.1,        # $0.10
        "ETH": 0.01,       # $0.01
        "SOL": 0.001,      # $0.001
        "AVAX": 0.001,     # $0.001
        "MATIC": 0.0001,   # $0.0001
        "ARB": 0.0001,     # $0.0001
        "OP": 0.0001,      # $0.0001
        "DOGE": 0.00001,   # $0.00001
        "SHIB": 0.000001,  # $0.000001
        "APE": 0.0001,     # $0.0001
        "LINK": 0.001,     # $0.001
        "UNI": 0.001,      # $0.001
        "AAVE": 0.01,      # $0.01
        "MKR": 0.1,        # $0.10
        "SNX": 0.001,      # $0.001
        "CRV": 0.0001,     # $0.0001
        "LDO": 0.001,      # $0.001
        "COMP": 0.01,      # $0.01
        "SUSHI": 0.0001,   # $0.0001
        "YFI": 0.1,        # $0.10
    }

def apply_critical_overrides(tick_sizes: Dict[str, float]) -> Dict[str, float]:
    """
    Apply critical overrides to tick sizes.
    Some assets may need special handling.
    
    Parameters:
    -----------
    tick_sizes : Dict[str, float]
        Dictionary mapping asset symbol to tick size
    
    Returns:
    --------
    Dict[str, float]
        Updated dictionary with overrides applied
    """
    # Critical overrides for specific assets
    overrides = {
        "BTC": 0.1,    # $0.10
        "ETH": 0.01,   # $0.01
        "SOL": 0.001,  # $0.001
    }
    
    # Apply overrides
    for symbol, tick_size in overrides.items():
        if symbol in tick_sizes and tick_sizes[symbol] != tick_size:
            logger.info(f"Overriding tick size for {symbol}: {tick_sizes[symbol]} -> {tick_size}")
        tick_sizes[symbol] = tick_size
    
    return tick_sizes

def round_to_tick_size(price: float, symbol: str, tick_sizes: Dict[str, float]) -> float:
    """
    Round price to the appropriate tick size for the symbol.
    
    Parameters:
    -----------
    price : float
        The price to round
    symbol : str
        The symbol/asset to round for
    tick_sizes : Dict[str, float]
        Dictionary mapping symbol to tick size
    
    Returns:
    --------
    float
        Rounded price
    """
    if not symbol in tick_sizes:
        # Use a default if symbol not found
        logger.warning(f"Tick size not found for {symbol}, using default of 0.01")
        tick_size = 0.01
    else:
        tick_size = tick_sizes[symbol]
    
    # Round to the nearest tick
    rounded_price = round(price / tick_size) * tick_size
    
    # Handle floating point precision issues
    # Convert to string with appropriate precision based on tick_size
    decimals = int(max(0, -math.log10(tick_size)))
    rounded_price = float(f"{rounded_price:.{decimals}f}")
    
    return rounded_price

def format_price(price: float, symbol: str, tick_sizes: Optional[Dict[str, float]] = None) -> str:
    """
    Format price for display with appropriate precision.
    
    Parameters:
    -----------
    price : float
        The price to format
    symbol : str
        The symbol/asset to format for
    tick_sizes : Optional[Dict[str, float]]
        Dictionary mapping symbol to tick size
    
    Returns:
    --------
    str
        Formatted price string
    """
    if tick_sizes and symbol in tick_sizes:
        tick_size = tick_sizes[symbol]
        # Determine decimal places based on tick size
        decimals = int(max(0, -math.log10(tick_size)))
        return f"${price:.{decimals}f}"
    
    # Default formatting based on common asset classes
    if symbol in ['BTC', 'ETH', 'MKR', 'YFI']:
        return f"${price:.2f}"
    elif symbol in ['SOL', 'AVAX', 'LINK', 'UNI', 'AAVE']:
        return f"${price:.3f}"
    elif price < 0.01:
        return f"${price:.6f}"
    elif price < 0.1:
        return f"${price:.5f}"
    elif price < 1:
        return f"${price:.4f}"
    elif price < 10:
        return f"${price:.3f}"
    else:
        return f"${price:.2f}"

def calculate_price_difference(price1: float, price2: float) -> float:
    """
    Calculate percentage difference between two prices.
    
    Parameters:
    -----------
    price1 : float
        First price
    price2 : float
        Second price
    
    Returns:
    --------
    float
        Percentage difference
    """
    if price1 == 0:
        return 0
    
    return (price2 - price1) / price1 * 100

def calculate_pnl(entry_price: float, exit_price: float, size: float, is_long: bool) -> float:
    """
    Calculate profit/loss for a trade.
    
    Parameters:
    -----------
    entry_price : float
        Entry price
    exit_price : float
        Exit price
    size : float
        Position size
    is_long : bool
        Whether the position is long
    
    Returns:
    --------
    float
        PnL amount
    """
    if is_long:
        return (exit_price - entry_price) * size
    else:
        return (entry_price - exit_price) * size