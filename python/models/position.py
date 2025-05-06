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
            order_fill_time=data.get("order_fill_time"),
            actual_entry_price=data.get("actual_entry_price"),
            last_updated=data.get("last_updated"),
            unrealized_pnl=data.get("unrealized_pnl", 0.0),
            mark_price=data.get("mark_price"),
            closed=data.get("closed", False),
            exit_price=data.get("exit_price"),
            exit_time=data.get("exit_time")
        )


@dataclass
class OpenOrder:
    """
    Represents an open order in the market.
    This is used for tracking limit orders that haven't been filled yet.
    """
    oid: str
    symbol: str
    signal_id: str
    is_long: bool
    entry_price: float
    size: float
    take_profit: float = 0.0
    stop_loss: float = 0.0
    timestamp: float = field(default_factory=time.time)
    order_type: str = "limit"
    
    @property
    def side(self) -> str:
        """Get the order side as a string"""
        return "LONG" if self.is_long else "SHORT"
    
    @property
    def age_hours(self) -> float:
        """Get the age of the order in hours"""
        current_time = time.time()
        return (current_time - self.timestamp) / 3600
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert order to dictionary for JSON serialization"""
        return {
            "oid": self.oid,
            "symbol": self.symbol,
            "signal_id": self.signal_id,
            "is_long": self.is_long,
            "side": self.side,
            "entry_price": self.entry_price,
            "size": self.size,
            "take_profit": self.take_profit,
            "stop_loss": self.stop_loss,
            "timestamp": self.timestamp,
            "order_type": self.order_type,
            "age_hours": self.age_hours
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'OpenOrder':
        """Create an OpenOrder from a dictionary"""
        return cls(
            oid=data.get("oid", ""),
            symbol=data.get("symbol", ""),
            signal_id=data.get("signal_id", "unknown"),
            is_long=data.get("is_long", True),
            entry_price=data.get("entry_price", 0.0),
            size=data.get("size", 0.0),
            take_profit=data.get("take_profit", 0.0),
            stop_loss=data.get("stop_loss", 0.0),
            timestamp=data.get("timestamp", time.time()),
            order_type=data.get("order_type", "limit")
        )


@dataclass
class PositionHistory:
    """
    Represents the history of a closed position.
    Used for tracking performance and generating statistics.
    """
    position_id: str
    signal_id: str
    symbol: str
    side: str
    entry_price: float
    exit_price: float
    size: float
    entry_time: float
    exit_time: float
    pnl: float
    pnl_percentage: float
    take_profit: Optional[float] = None
    stop_loss: Optional[float] = None
    exit_reason: str = "unknown"  # "tp", "sl", "manual", etc.
    
    @property
    def duration_hours(self) -> float:
        """Get the duration of the position in hours"""
        return (self.exit_time - self.entry_time) / 3600
    
    @property
    def is_profit(self) -> bool:
        """Check if the position was profitable"""
        return self.pnl > 0
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert position history to dictionary for JSON serialization"""
        return {
            "position_id": self.position_id,
            "signal_id": self.signal_id,
            "symbol": self.symbol,
            "side": self.side,
            "entry_price": self.entry_price,
            "exit_price": self.exit_price,
            "size": self.size,
            "entry_time": self.entry_time,
            "exit_time": self.exit_time,
            "pnl": self.pnl,
            "pnl_percentage": self.pnl_percentage,
            "take_profit": self.take_profit,
            "stop_loss": self.stop_loss,
            "exit_reason": self.exit_reason,
            "duration_hours": self.duration_hours,
            "is_profit": self.is_profit
        }
    
    @classmethod
    def from_position(cls, position: TradePosition, exit_price: float, exit_reason: str) -> 'PositionHistory':
        """
        Create a PositionHistory from a TradePosition and exit details.
        
        Parameters:
        -----------
        position : TradePosition
            The closed position
        exit_price : float
            The price at which the position was closed
        exit_reason : str
            The reason for closing the position
            
        Returns:
        --------
        PositionHistory
            The position history entry
        """
        # Calculate PnL
        entry_price = position.actual_entry_price or position.entry_price
        
        if position.is_long:
            pnl = (exit_price - entry_price) * position.current_size
            pnl_percentage = (exit_price - entry_price) / entry_price * 100
        else:
            pnl = (entry_price - exit_price) * position.current_size
            pnl_percentage = (entry_price - exit_price) / entry_price * 100
        
        return cls(
            position_id=position.position_id,
            signal_id=position.signal_id,
            symbol=position.symbol,
            side=position.side,
            entry_price=entry_price,
            exit_price=exit_price,
            size=position.current_size,
            entry_time=position.entry_time,
            exit_time=time.time(),
            pnl=pnl,
            pnl_percentage=pnl_percentage,
            take_profit=position.take_profit,
            stop_loss=position.stop_loss,
            exit_reason=exit_reason
        )