#!/usr/bin/env python3
"""
Signal models for the Hyperliquid trading bot.
Contains data models for trading signals and signal processing.
"""

import time
import uuid
from typing import Dict, Any, Optional, List
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum


class SignalSource(str, Enum):
    """Enum for signal sources"""
    MANUAL = "manual"
    AUTOMATION = "automation"
    API = "api"
    WEBHOOK = "webhook"
    BACKTEST = "backtest"
    OTHER = "other"


class SignalStatus(str, Enum):
    """Enum for signal processing status"""
    NEW = "new"
    PROCESSING = "processing"
    PROCESSED = "processed"
    IGNORED = "ignored"
    FAILED = "failed"
    EXPIRED = "expired"


@dataclass
class TradingSignal:
    """
    Represents a trading signal with entry and exit details.
    This is the core data structure for signal processing.
    """
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    symbol: str = ""
    position_type: str = "LONG"  # "LONG" or "SHORT"
    price: float = 0.0
    timestamp: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    take_profit: float = 0.0
    stop_loss: float = 0.0
    size: float = 0.0
    strength: float = 0.8  # Signal strength/confidence (0.0-1.0)
    source: SignalSource = SignalSource.MANUAL
    status: SignalStatus = SignalStatus.NEW
    metadata: Dict[str, Any] = field(default_factory=dict)
    processing_time: Optional[float] = None
    order_id: Optional[str] = None
    ignored_reason: Optional[str] = None
    error_message: Optional[str] = None
    
    @property
    def is_long(self) -> bool:
        """Check if the signal is for a long position"""
        return self.position_type.upper() == "LONG"
    
    @property
    def is_short(self) -> bool:
        """Check if the signal is for a short position"""
        return self.position_type.upper() == "SHORT"
    
    @property
    def age_minutes(self) -> float:
        """Get the age of the signal in minutes"""
        signal_time = datetime.fromisoformat(self.timestamp.replace('Z', '+00:00'))
        now_utc = datetime.now(timezone.utc)
        return (now_utc - signal_time).total_seconds() / 60
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert signal to dictionary for JSON serialization"""
        return {
            "id": self.id,
            "symbol": self.symbol,
            "position_type": self.position_type,
            "price": self.price,
            "timestamp": self.timestamp,
            "take_profit": self.take_profit,
            "stop_loss": self.stop_loss,
            "size": self.size,
            "strength": self.strength,
            "source": self.source,
            "status": self.status,
            "metadata": self.metadata,
            "processing_time": self.processing_time,
            "order_id": self.order_id,
            "ignored_reason": self.ignored_reason,
            "error_message": self.error_message,
            "age_minutes": self.age_minutes
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'TradingSignal':
        """Create a TradingSignal from a dictionary"""
        # Handle enums properly
        source = data.get("source", SignalSource.MANUAL)
        if isinstance(source, str):
            source = SignalSource(source)
            
        status = data.get("status", SignalStatus.NEW)
        if isinstance(status, str):
            status = SignalStatus(status)
            
        return cls(
            id=data.get("id", str(uuid.uuid4())),
            symbol=data.get("symbol", ""),
            position_type=data.get("position_type", "LONG"),
            price=data.get("price", 0.0),
            timestamp=data.get("timestamp", datetime.now(timezone.utc).isoformat()),
            take_profit=data.get("take_profit", 0.0),
            stop_loss=data.get("stop_loss", 0.0),
            size=data.get("size", 0.0),
            strength=data.get("strength", 0.8),
            source=source,
            status=status,
            metadata=data.get("metadata", {}),
            processing_time=data.get("processing_time"),
            order_id=data.get("order_id"),
            ignored_reason=data.get("ignored_reason"),
            error_message=data.get("error_message")
        )
    
    @classmethod
    def create_long_signal(cls, symbol: str, price: float, take_profit: float, stop_loss: float, size: float = 0.0, strength: float = 0.8) -> 'TradingSignal':
        """
        Create a long trading signal.
        
        Parameters:
        -----------
        symbol : str
            Trading symbol
        price : float
            Entry price
        take_profit : float
            Take profit price
        stop_loss : float
            Stop loss price
        size : float
            Position size (0.0 for automatic sizing)
        strength : float
            Signal strength/confidence (0.0-1.0)
            
        Returns:
        --------
        TradingSignal
            The created trading signal
        """
        return cls(
            symbol=symbol,
            position_type="LONG",
            price=price,
            take_profit=take_profit,
            stop_loss=stop_loss,
            size=size,
            strength=strength
        )
    
    @classmethod
    def create_short_signal(cls, symbol: str, price: float, take_profit: float, stop_loss: float, size: float = 0.0, strength: float = 0.8) -> 'TradingSignal':
        """
        Create a short trading signal.
        
        Parameters:
        -----------
        symbol : str
            Trading symbol
        price : float
            Entry price
        take_profit : float
            Take profit price
        stop_loss : float
            Stop loss price
        size : float
            Position size (0.0 for automatic sizing)
        strength : float
            Signal strength/confidence (0.0-1.0)
            
        Returns:
        --------
        TradingSignal
            The created trading signal
        """
        return cls(
            symbol=symbol,
            position_type="SHORT",
            price=price,
            take_profit=take_profit,
            stop_loss=stop_loss,
            size=size,
            strength=strength
        )


@dataclass
class SignalResult:
    """
    Represents the result of signal processing.
    Used for tracking the outcome of signal execution.
    """
    signal_id: str
    symbol: str
    position_type: str
    status: str  # "success", "error", "open_order", "ignored"
    message: str = ""
    order_id: Optional[str] = None
    entry_price: Optional[float] = None
    actual_entry_price: Optional[float] = None
    size: Optional[float] = None
    timestamp: float = field(default_factory=time.time)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert signal result to dictionary for JSON serialization"""
        return {
            "signal_id": self.signal_id,
            "symbol": self.symbol,
            "position_type": self.position_type,
            "status": self.status,
            "message": self.message,
            "order_id": self.order_id,
            "entry_price": self.entry_price,
            "actual_entry_price": self.actual_entry_price,
            "size": self.size,
            "timestamp": self.timestamp
        }


@dataclass
class SignalStats:
    """
    Statistics about signal processing and performance.
    Used for tracking signal metrics over time.
    """
    total_signals: int = 0
    processed_signals: int = 0
    ignored_signals: int = 0
    failed_signals: int = 0
    expired_signals: int = 0
    successful_signals: int = 0
    open_orders: int = 0
    profitable_signals: int = 0
    losing_signals: int = 0
    sources: Dict[str, int] = field(default_factory=dict)
    symbols: Dict[str, int] = field(default_factory=dict)
    
    def update_from_signal(self, signal: TradingSignal) -> None:
        """
        Update statistics from a signal.
        
        Parameters:
        -----------
        signal : TradingSignal
            The signal to update statistics from
        """
        self.total_signals += 1
        
        # Update status counts
        if signal.status == SignalStatus.PROCESSED:
            self.processed_signals += 1
        elif signal.status == SignalStatus.IGNORED:
            self.ignored_signals += 1
        elif signal.status == SignalStatus.FAILED:
            self.failed_signals += 1
        elif signal.status == SignalStatus.EXPIRED:
            self.expired_signals += 1
        
        # Update source counts
        source = signal.source.value
        self.sources[source] = self.sources.get(source, 0) + 1
        
        # Update symbol counts
        symbol = signal.symbol
        self.symbols[symbol] = self.symbols.get(symbol, 0) + 1
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert signal stats to dictionary for JSON serialization"""
        return {
            "total_signals": self.total_signals,
            "processed_signals": self.processed_signals,
            "ignored_signals": self.ignored_signals,
            "failed_signals": self.failed_signals,
            "expired_signals": self.expired_signals,
            "successful_signals": self.successful_signals,
            "open_orders": self.open_orders,
            "profitable_signals": self.profitable_signals,
            "losing_signals": self.losing_signals,
            "sources": self.sources,
            "symbols": self.symbols
        }