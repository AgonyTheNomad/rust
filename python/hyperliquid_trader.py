#!/usr/bin/env python3
"""
Hyperliquid Dashboard & Short Entry + TP/SL Market‐Trigger Script

• Loads creds from .env (HYPERLIQUID_API_SECRET / HYPERLIQUID_API_KEY)
• Prints spot USDC, perps withdrawable, account value, maintenance margin, cross‐margin ratio
• Places a SHORT limit at ENTRY_PRICE, waits for fill (polls or detects immediate fill)
• Then submits two **market‐trigger** orders:
    – Take‐Profit  @ TAKE_PROFIT (trigger when price ≤ TAKE_PROFIT, executes market)
    – Stop‐Loss    @ STOP_LOSS   (trigger when price ≥ STOP_LOSS, executes market)
• Dumps every raw response for inspection
"""

import os
import json
import asyncio
import logging

from dotenv import load_dotenv
from eth_account import Account
from hyperliquid.info import Info
from hyperliquid.exchange import Exchange
from hyperliquid.utils import constants

# ─── Parameters ────────────────────────────────────────────────────────────────
ENTRY_PRICE = 93140
TAKE_PROFIT = 91000
STOP_LOSS   = 95000
SIZE        = 0.04

# ─── Setup ─────────────────────────────────────────────────────────────────────
load_dotenv()
logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("hyperliquid_trader")

def get_account():
    secret  = os.getenv("HYPERLIQUID_API_SECRET")
    address = os.getenv("HYPERLIQUID_API_KEY")
    if not secret or not address:
        raise RuntimeError("Set HYPERLIQUID_API_SECRET & HYPERLIQUID_API_KEY in your .env")
    acct = Account.from_key(secret)
    print(f"Using on-chain address: {address}")
    if address != acct.address:
        print(f"  (API wallet key = {acct.address})")
    return acct, address

def get_api_url():
    return (
        constants.TESTNET_API_URL
        if os.getenv("HYPERLIQUID_TESTNET","true").lower()=="true"
        else constants.MAINNET_API_URL
    )

async def with_retries(fn, *args, retries=3, backoff=1, **kwargs):
    attempt=0
    while True:
        try:
            r = fn(*args, **kwargs)
            return await r if asyncio.iscoroutine(r) else r
        except Exception as e:
            attempt+=1
            if attempt>retries:
                logger.error(f"{fn.__name__} failed after {retries} retries: {e}")
                raise
            wait = backoff*(2**(attempt-1))
            logger.warning(f"{fn.__name__} error ({e}), retrying in {wait:.1f}s…")
            await asyncio.sleep(wait)

async def wait_for_fill(info: Info, address: str, oid: int):
    while True:
        resp   = await with_retries(info.query_order_by_oid, address, oid)
        status = resp.get("order",{}).get("status")
        logger.info(f"Order {oid} status: {status}")
        if status=="filled":
            return resp
        await asyncio.sleep(1.0)

async def print_dashboard(info: Info, address: str):
    spot  = await with_retries(info.spot_user_state, address)
    for b in spot.get("balances",[]):
        if b["coin"]=="USDC":
            avail = float(b["total"])-float(b["hold"])
            logger.info(f"Spot USDC Available:       ${avail:.2f}")
            break
    else:
        logger.info("Spot USDC Available:       $0.00")

    state = await with_retries(info.user_state, address)
    w     = float(state.get("withdrawable",0))
    cms   = state["crossMarginSummary"]
    av    = float(cms["accountValue"])
    mm    = float(state.get("crossMaintenanceMarginUsed",0))
    ratio = mm/av if av else 0.0

    logger.info(f"Perps Withdrawable:        ${w:.2f}")
    logger.info(f"Account Value:             ${av:.2f}")
    logger.info(f"Maintenance Margin:        ${mm:.2f}")
    logger.info(f"Cross-Margin Ratio:        {ratio:.2%}")

async def main():
    acct, address = get_account()
    api_url       = get_api_url()
    info          = Info(api_url, skip_ws=True)
    exchange      = Exchange(acct, api_url, account_address=address)

    # 1) Dashboard
    await print_dashboard(info, address)

    # 2) Entry limit
    logger.info(f"Placing SHORT limit: size={SIZE}, price={ENTRY_PRICE}")
    entry = exchange.order(
        "BTC", False, SIZE, ENTRY_PRICE,
        {"limit":{"tif":"Gtc"}},
        reduce_only=False
    )
    logger.info("Entry response:\n%s", json.dumps(entry,indent=2))

    statuses    = entry["response"]["data"]["statuses"]
    resting_oid = next((s["resting"]["oid"] for s in statuses if "resting" in s),None)
    filled_oid  = next((s["filled"]["oid"]  for s in statuses if "filled"  in s),None)

    if resting_oid:
        oid         = resting_oid
        filled_resp = await wait_for_fill(info, address, oid)
    else:
        oid         = filled_oid
        filled_resp = await with_retries(info.query_order_by_oid, address, oid)

    logger.info("Entry filled:\n%s", json.dumps(filled_resp,indent=2))

    # 3) Take-Profit market-trigger
    tp_resp = exchange.order(
        "BTC", True, SIZE, ENTRY_PRICE,
        {"trigger":{"tpsl":"tp","triggerPx":TAKE_PROFIT,"isMarket":True}},
        reduce_only=True
    )
    logger.info("TP response:\n%s", json.dumps(tp_resp,indent=2))

    # 4) Stop-Loss market-trigger
    sl_resp = exchange.order(
        "BTC", True, SIZE, ENTRY_PRICE,
        {"trigger":{"tpsl":"sl","triggerPx":STOP_LOSS,"isMarket":True}},
        reduce_only=True
    )
    logger.info("SL response:\n%s", json.dumps(sl_resp,indent=2))

if __name__=="__main__":
    asyncio.run(main())
