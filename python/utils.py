#!/usr/bin/env python3
"""
Utility functions for Hyperliquid Trader
"""

import os
from dotenv import load_dotenv
from eth_account import Account
from eth_account.signers.local import LocalAccount
from hyperliquid.info import Info
from hyperliquid.exchange import Exchange
from hyperliquid.utils import constants

# Load .env into os.environ
load_dotenv()  

def setup(skip_ws: bool = False):
    """
    Builds (address, info, exchange) entirely from ENV vars loaded from .env.
    
    Returns:
        Tuple of (address, info, exchange) for interacting with Hyperliquid
    """
    # 1) Get private key and derive account
    priv_key = os.getenv("HYPERLIQUID_API_SECRET")
    if not priv_key:
        raise RuntimeError("HYPERLIQUID_API_SECRET not set in environment")
    acct: LocalAccount = Account.from_key(priv_key)

    # 2) Public address (agent key vs account)
    address = os.getenv("HYPERLIQUID_API_KEY", acct.address)

    print(f"Running with account address: {address}")
    if address != acct.address:
        print(f"Using agent key address: {acct.address}")

    # 3) Choose network
    use_test = os.getenv("HYPERLIQUID_TESTNET", "true").lower() == "true"
    api_url = constants.TESTNET_API_URL if use_test else constants.MAINNET_API_URL

    # 4) Instantiate Info & Exchange
    info = Info(api_url, skip_ws)
    exchange = Exchange(acct, api_url, account_address=address)

    return address, info, exchange