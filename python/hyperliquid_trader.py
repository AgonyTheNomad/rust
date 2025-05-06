#!/usr/bin/env python3
"""
Hyperliquid Trader - Modular Version

This script is the entry point for the Hyperliquid trading bot.
It initializes the trading system and starts the main loop.
"""

import asyncio
import argparse
import logging
from pathlib import Path
from dotenv import load_dotenv

from trading.trader import HyperliquidTrader

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

def parse_args():
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(description="Hyperliquid Trader - Modular Version")
    
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
    # Load environment variables
    load_dotenv()
    
    logger.info("Starting Hyperliquid Trader - Modular Version")
    args = parse_args()
    
    # Create directories if they don't exist
    Path(args.signals).mkdir(exist_ok=True)
    Path(args.archive).mkdir(exist_ok=True)
    Path(args.commands).mkdir(exist_ok=True)
    
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