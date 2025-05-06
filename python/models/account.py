#!/usr/bin/env python3
"""
Account models for the Hyperliquid trading bot.
Contains data models for account information and positions.
"""

import time
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, field


@dataclass
class Position:
    """
    Represents a trading position on Hyperliquid.
    """
    symbol: str
    size: float
    entry_price: float
    side: str  # "LONG" or "SHORT"
    unrealized_pnl: float = 0.0
    mark_price: Optional[float] = None
    liquidation_price: Optional[float] = None
    
    @property
    def is_long(self) -> bool:
        """Check if the position is long"""
        return self.side.upper() == "LONG"
    
    @property
    def is_short(self) -> bool:
        """Check if the position is short"""
        return self.side.upper() == "SHORT"
    
    @property
    def percentage_pnl(self) -> float:
        """Calculate the percentage PnL of the position"""
        if self.entry_price <= 0 or not self.mark_price:
            return 0.0
        
        if self.is_long:
            return (self.mark_price - self.entry_price) / self.entry_price * 100
        else:
            return (self.entry_price - self.mark_price) / self.entry_price * 100
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert position to dictionary for JSON serialization"""
        return {
            "symbol": self.symbol,
            "size": self.size,
            "entry_price": self.entry_price,
            "side": self.side,
            "unrealized_pnl": self.unrealized_pnl,
            "mark_price": self.mark_price,
            "liquidation_price": self.liquidation_price,
            "percentage_pnl": self.percentage_pnl
        }


@dataclass
class AccountInfo:
    """
    Represents account information from Hyperliquid.
    """
    balance: float = 0.0
    available_margin: float = 0.0
    used_margin: float = 0.0
    timestamp: float = field(default_factory=time.time)
    positions: List[Position] = field(default_factory=list)
    
    @classmethod
    def from_exchange_data(cls, state: Dict[str, Any]) -> 'AccountInfo':
        """
        Create an AccountInfo instance from exchange state data.
        
        Parameters:
        -----------
        state : Dict[str, Any]
            User state data from the exchange
            
        Returns:
        --------
        AccountInfo
            Parsed account information
        """
        account_info = cls()
        
        if isinstance(state, dict):
            # Extract account value
            if "crossMarginSummary" in state:
                cross_margin_summary = state["crossMarginSummary"]
                account_info.balance = float(cross_margin_summary.get("accountValue", 0))
            
            # Extract margin information
            account_info.available_margin = float(state.get("withdrawable", 0))
            account_info.used_margin = float(state.get("crossMaintenanceMarginUsed", 0))
            account_info.timestamp = time.time()
            
            # Extract positions
            if "assetPositions" in state:
                for pos in state["assetPositions"]:
                    if isinstance(pos, dict) and "coin" in pos:
                        size = float(pos.get("szi", 0))
                        if abs(size) > 0:
                            entry_px = float(pos.get("entryPx", 0))
                            upnl = float(pos.get("unrealizedPnl", 0))
                            side = "LONG" if size > 0 else "SHORT"
                            mark_price = float(pos.get("markPx", entry_px))
                            liquidation_price = float(pos.get("liquidationPx", 0))
                            
                            position = Position(
                                symbol=pos["coin"],
                                size=abs(size),
                                entry_price=entry_px,
                                side=side,
                                unrealized_pnl=upnl,
                                mark_price=mark_price,
                                liquidation_price=liquidation_price
                            )
                            account_info.positions.append(position)
        
        return account_info
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert account info to dictionary for JSON serialization"""
        return {
            "balance": self.balance,
            "available_margin": self.available_margin,
            "used_margin": self.used_margin,
            "timestamp": self.timestamp,
            "positions": [pos.to_dict() for pos in self.positions]
        }


@dataclass
class MarginInfo:
    """
    Detailed margin information for the account.
    """
    initial_margin: float = 0.0
    maintenance_margin: float = 0.0
    margin_ratio: float = 0.0
    leverage: float = 1.0
    
    @property
    def margin_level(self) -> str:
        """Get the margin safety level"""
        if self.margin_ratio <= 0.5:
            return "SAFE"
        elif self.margin_ratio <= 0.75:
            return "WARNING"
        elif self.margin_ratio <= 0.9:
            return "DANGER"
        else:
            return "CRITICAL"
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert margin info to dictionary for JSON serialization"""
        return {
            "initial_margin": self.initial_margin,
            "maintenance_margin": self.maintenance_margin,
            "margin_ratio": self.margin_ratio,
            "leverage": self.leverage,
            "margin_level": self.margin_level
        }