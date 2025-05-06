#!/usr/bin/env python3
"""
Position models for the Hyperliquid trading bot.
Contains data models for trading positions and orders.
"""

import time
from typing import Dict, Any, Optional, List
from dataclasses import dataclass, field
from enum import Enum


class PositionSide(str, Enum):
    """Enum for position side"""
    LONG = "LONG"
    SHORT = "SHORT"


class OrderStatus(str, Enum):
    """Enum for order status"""
    OPEN = "open"
    FILLED = "filled"
    CANCELED = "canceled"
    REJECTED = "rejected"
    UNKNOWN = "unknown"


@dataclass
class TradePosition:
    """
    Represents a trade position with entry and exit details.
    This is used for internal tracking of positions.
    """
    signal_id: str
    symbol: str
    is_long: bool
    entry_price: float
    current_size: float
    take_profit: float = 0.0
    stop_loss: float = 0.0
    entry_time: float = field(default_factory=time.time)
    order_fill_time: Optional[float] = None
    actual_entry_price: Optional[float] = None
    last_updated: Optional[float] = None
    unrealized_pnl: float = 0.0
    mark_price: Optional[float] = None
    closed: bool = False
    exit_price: Optional[float] = None
    exit_time: Optional[float] = None
    
    @property
    def side(self) -> str:
        """Get the position side as a string"""
        return "LONG" if self.is_long else "SHORT"
    
    @property
    def position_id(self) -> str:
        """Get a unique identifier for the position"""
        return f"{self.symbol}_{self.signal_id}"
    
    @property
    def age_hours(self) -> float:
        """Get the age of the position in hours"""
        current_time = time.time()
        return (current_time - self.entry_time) / 3600
    
    @property
    def is_in_profit(self) -> bool:
        """Check if the position is currently in profit"""
        if not self.mark_price or not self.actual_entry_price:
            return False
        
        if self.is_long:
            return self.mark_price > self.actual_entry_price
        else:
            return self.mark_price < self.actual_entry_price
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert position to dictionary for JSON serialization"""
        return {
            "signal_id": self.signal_id,
            "symbol": self.symbol,
            "is_long": self.is_long,
            "side": self.side,
            "entry_price": self.entry_price,
            "actual_entry_price": self.actual_entry_price,
            "current_size": self.current_size,
            "take_profit": self.take_profit,
            "stop_loss": self.stop_loss,
            "entry_time": self.entry_time,
            "order_fill_time": self.order_fill_time,
            "last_updated": self.last_updated,
            "unrealized_pnl": self.unrealized_pnl,
            "mark_price": self.mark_price,
            "closed": self.closed,
            "exit_price": self.exit_price,
            "exit_time": self.exit_time,
            "age_hours": self.age_hours,
            "is_in_profit": self.is_in_profit
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'TradePosition':
        """Create a TradePosition from a dictionary"""
        return cls(
            signal_id=data.get("signal_id", "unknown"),
            symbol=data.get("symbol", ""),
            is_long=data.get("is_long", True),
            entry_price=data.get("entry_price", 0.0),
            current_size=data.get("current_size", 0.0),
            take_profit=data.get("take_profit", 0.0),
            stop_loss=data.get("stop_loss", 0.0),
            entry_time=data.get("entry_time", time.time()),
            order_fill_time=data.get