use super::{Exchange, ExchangeConfig, ExchangeError, Order, OrderBook, OrderSide, OrderStatus, OrderType};
use crate::models::{Position, PositionType, Trade, ExitReason};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use log::*;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct HyperliquidExchange {
    config: ExchangeConfig,
    client: Client,
    pub influx: crate::influxdb::InfluxDBClient,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidTickerResponse {
    symbol: String,
    price: String,
    timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidOrderBookEntry {
    price: String,
    size: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidOrderBookResponse {
    bids: Vec<HyperliquidOrderBookEntry>,
    asks: Vec<HyperliquidOrderBookEntry>,
    timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidBalanceResponse {
    total: String,
    available: String,
    used: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidPositionResponse {
    symbol: String,
    side: String,
    entryPrice: String,
    markPrice: String,
    positionAmt: String,
    unrealizedProfit: String,
    leverage: String,
    marginType: String,
    isolatedMargin: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidOrderResponse {
    orderId: String,
    symbol: String,
    status: String,
    clientOrderId: Option<String>,
    price: String,
    avgPrice: String,
    origQty: String,
    executedQty: String,
    cumQuote: String,
    timeInForce: String,
    type_field: String,
    #[serde(rename = "type")]
    order_type: String,
    side: String,
    stopPrice: Option<String>,
    time: i64,
    updateTime: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HyperliquidAccountInfo {
    feeTier: u32,
    canTrade: bool,
    canDeposit: bool,
    canWithdraw: bool,
    updateTime: i64,
    totalInitialMargin: String,
    totalMaintMargin: String,
    totalWalletBalance: String,
    totalUnrealizedProfit: String,
    totalMarginBalance: String,
    totalPositionInitialMargin: String,
    totalOpenOrderInitialMargin: String,
    totalCrossWalletBalance: String,
    availableBalance: String,
    maxWithdrawAmount: String,
}

impl HyperliquidExchange {
    pub fn new(config: ExchangeConfig, influx_client: crate::influxdb::InfluxDBClient) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { 
            config, 
            client,
            influx: influx_client,
        })
    }

    fn sign_request(&self, payload: &str, timestamp: u64) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(format!("{}{}", timestamp, payload).as_bytes());
        
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        
        hex::encode(code_bytes)
    }

    async fn make_signed_request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        endpoint: &str,
        params: Option<serde_json::Value>,
    ) -> Result<T, ExchangeError> {
        let url = format!("{}{}", self.config.base_url, endpoint);
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let payload = match params {
            Some(p) => serde_json::to_string(&p).unwrap_or_default(),
            None => String::new(),
        };
        
        let signature = self.sign_request(&payload, timestamp);
        
        let response = match method {
            "GET" => {
                self.client.get(&url)
                    .header("X-HL-APIKEY", &self.config.api_key)
                    .header("X-HL-TIMESTAMP", timestamp.to_string())
                    .header("X-HL-SIGNATURE", signature)
                    .send()
                    .await
            },
            "POST" => {
                self.client.post(&url)
                    .header("X-HL-APIKEY", &self.config.api_key)
                    .header("X-HL-TIMESTAMP", timestamp.to_string())
                    .header("X-HL-SIGNATURE", signature)
                    .json(&params.unwrap_or(serde_json::json!({})))
                    .send()
                    .await
            },
            "DELETE" => {
                self.client.delete(&url)
                    .header("X-HL-APIKEY", &self.config.api_key)
                    .header("X-HL-TIMESTAMP", timestamp.to_string())
                    .header("X-HL-SIGNATURE", signature)
                    .send()
                    .await
            },
            _ => return Err(ExchangeError::UnknownError(format!("Unsupported method: {}", method))),
        };
        
        match response {
            Ok(res) => {
                let status = res.status();
                let body = res.text().await.unwrap_or_else(|_| String::from(""));

                if status == StatusCode::OK {
                    match serde_json::from_str::<HyperliquidResponse<T>>(&body) {
                        Ok(hl_response) => {
                            if hl_response.success {
                                match hl_response.data {
                                    Some(data) => Ok(data),
                                    None => Err(ExchangeError::ApiError("No data in response".to_string())),
                                }
                            } else {
                                Err(ExchangeError::ApiError(hl_response.error.unwrap_or_else(|| "Unknown API error".to_string())))
                            }
                        },
                        Err(e) => {
                            error!("Failed to parse Hyperliquid response: {}, Body: {}", e, body);
                            Err(ExchangeError::ParseError(format!("Failed to parse response: {}", e)))
                        },
                    }
                } else if status == StatusCode::UNAUTHORIZED {
                    Err(ExchangeError::AuthError("Invalid API key or signature".to_string()))
                } else if status == StatusCode::TOO_MANY_REQUESTS {
                    Err(ExchangeError::RateLimitExceeded("Rate limit exceeded".to_string()))
                } else {
                    Err(ExchangeError::ApiError(format!("HTTP error {}: {}", status, body)))
                }
            },
            Err(e) => Err(ExchangeError::NetworkError(format!("Request failed: {}", e))),
        }
    }

    fn map_hyperliquid_order_status(&self, status: &str) -> OrderStatus {
        match status {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Canceled,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::New,
        }
    }

    fn map_hyperliquid_order_type(&self, order_type: &str) -> OrderType {
        match order_type {
            "MARKET" => OrderType::Market,
            "LIMIT" => OrderType::Limit,
            "STOP" | "STOP_MARKET" => OrderType::StopLoss,
            "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
            "TRAILING_STOP_MARKET" => OrderType::TrailingStop,
            _ => OrderType::Market,
        }
    }

    fn map_hyperliquid_order_side(&self, side: &str) -> OrderSide {
        match side {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => OrderSide::Buy,
        }
    }

    fn map_order_type_to_hyperliquid(&self, order_type: OrderType) -> &'static str {
        match order_type {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::StopLoss => "STOP_MARKET",
            OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
            OrderType::TrailingStop => "TRAILING_STOP_MARKET",
        }
    }

    fn map_order_side_to_hyperliquid(&self, side: OrderSide) -> &'static str {
        match side {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }
    
    async fn get_account_info_internal(&self) -> Result<HyperliquidAccountInfo, ExchangeError> {
        let endpoint = "/api/v1/account";
        self.make_signed_request("GET", endpoint, None).await
    }
}

#[async_trait]
impl Exchange for HyperliquidExchange {
    async fn get_name(&self) -> &str {
        &self.config.name
    }
    
    async fn get_ticker(&self, symbol: &str) -> Result<f64, ExchangeError> {
        let endpoint = format!("/api/v1/ticker/price?symbol={}", symbol);
        
        let response: HyperliquidTickerResponse = self.make_signed_request("GET", &endpoint, None).await?;
        
        match response.price.parse::<f64>() {
            Ok(price) => Ok(price),
            Err(_) => Err(ExchangeError::ParseError(format!("Failed to parse price: {}", response.price))),
        }
    }
    
    async fn get_order_book(&self, symbol: &str, depth: Option<usize>) -> Result<OrderBook, ExchangeError> {
        let limit = depth.unwrap_or(20).min(1000);
        let endpoint = format!("/api/v1/depth?symbol={}&limit={}", symbol, limit);
        
        let response: HyperliquidOrderBookResponse = self.make_signed_request("GET", &endpoint, None).await?;
        
        let bids = response.bids.iter()
            .filter_map(|entry| {
                match (entry.price.parse::<f64>(), entry.size.parse::<f64>()) {
                    (Ok(price), Ok(size)) => Some((price, size)),
                    _ => None,
                }
            })
            .collect();
            
        let asks = response.asks.iter()
            .filter_map(|entry| {
                match (entry.price.parse::<f64>(), entry.size.parse::<f64>()) {
                    (Ok(price), Ok(size)) => Some((price, size)),
                    _ => None,
                }
            })
            .collect();
            
        let timestamp = DateTime::<Utc>::from_timestamp(response.timestamp / 1000, 0)
            .unwrap_or_else(|| Utc::now());
            
        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids,
            asks,
            timestamp,
        })
    }
    
    async fn get_balance(&self) -> Result<f64, ExchangeError> {
        let account_info = self.get_account_info_internal().await?;
        
        match account_info.availableBalance.parse::<f64>() {
            Ok(balance) => Ok(balance),
            Err(_) => Err(ExchangeError::ParseError(
                format!("Failed to parse balance: {}", account_info.availableBalance)
            )),
        }
    }
    
    async fn get_account_info(&self) -> Result<crate::models::Account, ExchangeError> {
        let account_info = self.get_account_info_internal().await?;
        
        let wallet_balance = account_info.totalWalletBalance.parse::<f64>()
            .map_err(|_| ExchangeError::ParseError("Failed to parse wallet balance".to_string()))?;
            
        let unrealized_profit = account_info.totalUnrealizedProfit.parse::<f64>()
            .unwrap_or(0.0);
            
        let margin_balance = account_info.totalMarginBalance.parse::<f64>()
            .unwrap_or(wallet_balance);
            
        let total_initial_margin = account_info.totalInitialMargin.parse::<f64>()
            .unwrap_or(0.0);
        
        let positions = self.get_positions().await?;
        let mut position_map = HashMap::new();
        for pos in positions {
            position_map.insert(pos.id.clone(), pos);
        }
        
        Ok(crate::models::Account {
            balance: wallet_balance,
            equity: wallet_balance + unrealized_profit,
            used_margin: total_initial_margin,
            positions: position_map,
        })
    }
    
    async fn get_positions(&self) -> Result<Vec<Position>, ExchangeError> {
        let endpoint = "/api/v1/positionRisk";
        
        let positions: Vec<HyperliquidPositionResponse> = self.make_signed_request("GET", endpoint, None).await?;
        
        let result = positions
            .into_iter()
            .filter(|pos| pos.positionAmt.parse::<f64>().unwrap_or(0.0) != 0.0)
            .map(|pos| {
                let position_type = if pos.side == "LONG" { 
                    PositionType::Long 
                } else { 
                    PositionType::Short 
                };
                
                let entry_price = pos.entryPrice.parse::<f64>().unwrap_or(0.0);
                let mark_price = pos.markPrice.parse::<f64>().unwrap_or(0.0);
                let size = pos.positionAmt.parse::<f64>().unwrap_or(0.0).abs();
                
                Position {
                    id: format!("hl_{}", pos.symbol),
                    symbol: pos.symbol,
                    entry_time: Utc::now(), // API doesn't provide entry time
                    entry_price,
                    size,
                    stop_loss: 0.0, // Not provided by API
                    take_profit: 0.0, // Not provided by API
                    position_type,
                    risk_percent: 0.0,
                    margin_used: pos.isolatedMargin.parse::<f64>().unwrap_or(0.0),
                    status: crate::models::PositionStatus::Open,
                    limit1_price: None,
                    limit2_price: None,
                    limit1_hit: false,
                    limit2_hit: false,
                    limit1_size: 0.0,
                    limit2_size: 0.0,
                    new_tp1: None,
                    new_tp2: None,
                    entry_order_id: None,
                    tp_order_id: None,
                    sl_order_id: None,
                    limit1_order_id: None,
                    limit2_order_id: None,
                }
            })
            .collect();
            
        Ok(result)
    }
    
    async fn create_order(&self, order: Order) -> Result<Order, ExchangeError> {
        let endpoint = "/api/v1/order";
        
        let mut params = serde_json::json!({
            "symbol": order.symbol,
            "side": self.map_order_side_to_hyperliquid(order.side),
            "type": self.map_order_type_to_hyperliquid(order.order_type),
            "quantity": order.amount.to_string(),
        });
        
        if let Some(price) = order.price {
            params["price"] = price.to_string().into();
        }
        
        if order.order_type == OrderType::StopLoss || order.order_type == OrderType::TakeProfit || order.order_type == OrderType::TrailingStop {
            if let Some(price) = order.price {
                params["stopPrice"] = price.to_string().into();
            } else {
                return Err(ExchangeError::InvalidOrder("Stop price required for stop orders".to_string()));
            }
        }
        
        let response: HyperliquidOrderResponse = self.make_signed_request("POST", endpoint, Some(params)).await?;
        
        let mut updated_order = order.clone();
        updated_order.exchange_id = Some(response.orderId);
        updated_order.status = self.map_hyperliquid_order_status(&response.status);
        updated_order.filled_amount = response.executedQty.parse::<f64>().unwrap_or(0.0);
        updated_order.updated_at = Some(Utc::now());
        
        Ok(updated_order)
    }
    
    async fn cancel_order(&self, order_id: &str, symbol: &str) -> Result<(), ExchangeError> {
        let endpoint = "/api/v1/order";
        
        let params = serde_json::json!({
            "symbol": symbol,
            "orderId": order_id,
        });
        
        let _: serde_json::Value = self.make_signed_request("DELETE", endpoint, Some(params)).await?;
        
        Ok(())
    }
    
    async fn get_order(&self, order_id: &str, symbol: &str) -> Result<Order, ExchangeError> {
        let endpoint = format!("/api/v1/order?symbol={}&orderId={}", symbol, order_id);
        
        let response: HyperliquidOrderResponse = self.make_signed_request("GET", &endpoint, None).await?;
        
        let order = Order {
            id: uuid::Uuid::new_v4().to_string(), // Generate a new ID since we don't have the original
            exchange_id: Some(response.orderId),
            symbol: response.symbol,
            order_type: self.map_hyperliquid_order_type(&response.order_type),
            side: self.map_hyperliquid_order_side(&response.side),
            price: response.price.parse::<f64>().ok(),
            amount: response.origQty.parse::<f64>().unwrap_or(0.0),
            filled_amount: response.executedQty.parse::<f64>().unwrap_or(0.0),
            status: self.map_hyperliquid_order_status(&response.status),
            created_at: DateTime::<Utc>::from_timestamp(response.time / 1000, 0).unwrap_or_else(|| Utc::now()),
            updated_at: Some(DateTime::<Utc>::from_timestamp(response.updateTime / 1000, 0).unwrap_or_else(|| Utc::now())),
            position_id: None,
        };
        
        Ok(order)
    }
    
    async fn open_position(&self, position: &Position) -> Result<Position, ExchangeError> {
        // Step 1: Create market order to open position
        let side = self.position_type_to_order_side(&position.position_type);
        
        let order = Order {
            id: uuid::Uuid::new_v4().to_string(),
            exchange_id: None,
            symbol: position.symbol.clone(),
            order_type: OrderType::Market,
            side,
            price: None, // Market order, no price
            amount: position.size,
            filled_amount: 0.0,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: None,
            position_id: Some(position.id.clone()),
        };
        
        let entry_order = self.create_order(order).await?;
        
        if entry_order.status != OrderStatus::Filled {
            return Err(ExchangeError::InvalidOrder(format!("Entry order not filled: {:?}", entry_order.status)));
        }
        
        // Step 2: Create stop loss order
        let stop_side = self.close_position_side(&position.position_type);
        
        let sl_order = Order {
            id: uuid::Uuid::new_v4().to_string(),
            exchange_id: None,
            symbol: position.symbol.clone(),
            order_type: OrderType::StopLoss,
            side: stop_side,
            price: Some(position.stop_loss),
            amount: position.size,
            filled_amount: 0.0,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: None,
            position_id: Some(position.id.clone()),
        };
        
        let sl_order = self.create_order(sl_order).await?;
        
        // Step 3: Create take profit order
        let tp_order = Order {
            id: uuid::Uuid::new_v4().to_string(),
            exchange_id: None,
            symbol: position.symbol.clone(),
            order_type: OrderType::TakeProfit,
            side: stop_side,
            price: Some(position.take_profit),
            amount: position.size,
            filled_amount: 0.0,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: None,
            position_id: Some(position.id.clone()),
        };
        
        let tp_order = self.create_order(tp_order).await?;
        
        // Step 4: Create limit orders if defined
        let mut limit1_order_id = None;
        let mut limit2_order_id = None;
        
        if let Some(limit1_price) = position.limit1_price {
            if position.limit1_size > 0.0 {
                let limit1_order = Order {
                    id: uuid::Uuid::new_v4().to_string(),
                    exchange_id: None,
                    symbol: position.symbol.clone(),
                    order_type: OrderType::Limit,
                    side,
                    price: Some(limit1_price),
                    amount: position.limit1_size,
                    filled_amount: 0.0,
                    status: OrderStatus::New,
                    created_at: Utc::now(),
                    updated_at: None,
                    position_id: Some(position.id.clone()),
                };
                
                let limit1_result = self.create_order(limit1_order).await?;
                limit1_order_id = limit1_result.exchange_id;
            }
        }
        
        if let Some(limit2_price) = position.limit2_price {
            if position.limit2_size > 0.0 {
                let limit2_order = Order {
                    id: uuid::Uuid::new_v4().to_string(),
                    exchange_id: None,
                    symbol: position.symbol.clone(),
                    order_type: OrderType::Limit,
                    side,
                    price: Some(limit2_price),
                    amount: position.limit2_size,
                    filled_amount: 0.0,
                    status: OrderStatus::New,
                    created_at: Utc::now(),
                    updated_at: None,
                    position_id: Some(position.id.clone()),
                };
                
                let limit2_result = self.create_order(limit2_order).await?;
                limit2_order_id = limit2_result.exchange_id;
            }
        }
        
        // Create updated position to return
        let mut updated_position = position.clone();
        updated_position.entry_order_id = entry_order.exchange_id;
        updated_position.sl_order_id = sl_order.exchange_id;
        updated_position.tp_order_id = tp_order.exchange_id;
        updated_position.limit1_order_id = limit1_order_id;
        updated_position.limit2_order_id = limit2_order_id;
        updated_position.status = crate::models::PositionStatus::Open;
        
        Ok(updated_position)
    }
    
    async fn close_position(&self, position_id: &str) -> Result<Trade, ExchangeError> {
        // First get the position
        let positions = self.get_positions().await?;
        
        let position = positions.iter()
            .find(|p| p.id == position_id)
            .ok_or(ExchangeError::OrderNotFound(format!("Position not found: {}", position_id)))?;
        
        // Cancel any pending orders for this position
        if let Some(sl_id) = &position.sl_order_id {
            let _ = self.cancel_order(sl_id, &position.symbol).await;
        }
        
        if let Some(tp_id) = &position.tp_order_id {
            let _ = self.cancel_order(tp_id, &position.symbol).await;
        }
        
        if let Some(limit1_id) = &position.limit1_order_id {
            if !position.limit1_hit {
                let _ = self.cancel_order(limit1_id, &position.symbol).await;
            }
        }
        
        if let Some(limit2_id) = &position.limit2_order_id {
            if !position.limit2_hit {
                let _ = self.cancel_order(limit2_id, &position.symbol).await;
            }
        }
        
        // Create market order to close position
        let close_side = self.close_position_side(&position.position_type);
        
        let order = Order {
            id: uuid::Uuid::new_v4().to_string(),
            exchange_id: None,
            symbol: position.symbol.clone(),
            order_type: OrderType::Market,
            side: close_side,
            price: None,
            amount: position.size,
            filled_amount: 0.0,
            status: OrderStatus::New,
            created_at: Utc::now(),
            updated_at: None,
            position_id: Some(position.id.clone()),
        };
        
        let close_order = self.create_order(order).await?;
        
        if close_order.status != OrderStatus::Filled {
            return Err(ExchangeError::InvalidOrder(
                format!("Close order not filled: {:?}", close_order.status)
            ));
        }
        
        // Get current price for P&L calculation
        let current_price = self.get_ticker(&position.symbol).await?;
        
        // Calculate P&L
        let pnl = match position.position_type {
            PositionType::Long => (current_price - position.entry_price) * position.size,
            PositionType::Short => (position.entry_price - current_price) * position.size,
        };
        
        // Create trade record
        let trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            position_id: position.id.clone(),
            symbol: position.symbol.clone(),
            entry_time: position.entry_time,
            exit_time: Utc::now(),
            position_type: position.position_type.to_string(),
            entry_price: position.entry_price,
            exit_price: current_price,
            size: position.size,
            pnl,
            risk_percent: position.risk_percent,
            profit_factor: if pnl > 0.0 { pnl / (position.size * position.entry_price) } else { 0.0 },
            margin_used: position.margin_used,
            fees: 0.0, // Could calculate based on position size & exchange fee rate
            slippage: 0.0,
            exit_reason: ExitReason::ManualClose,
        };
        
        // Log trade to InfluxDB
        let _ = self.influx.write_trade(&trade).await;
        
        Ok(trade)
    }
    
    async fn update_position(&self, position: &Position) -> Result<Position, ExchangeError> {
        // This method would handle things like:
        // 1. Updating stop loss
        // 2. Updating take profit
        // 3. Handling limit order hits
        
        // First check if the position exists
        let positions = self.get_positions().await?;
        
        if !positions.iter().any(|p| p.id == position.id) {
            return Err(ExchangeError::OrderNotFound(format!("Position not found: {}", position.id)));
        }
        
        let mut updated_position = position.clone();
        
        // If stop loss changed, update the order
        if let Some(sl_id) = &position.sl_order_id {
            let sl_order = self.get_order(sl_id, &position.symbol).await?;
            
            // Check if we need to update the stop loss
            let sl_price = sl_order.price.unwrap_or(0.0);
            if sl_price != position.stop_loss {
                // Cancel existing stop loss
                self.cancel_order(sl_id, &position.symbol).await?;
                
                // Create new stop loss
                let stop_side = self.close_position_side(&position.position_type);
                
                let new_sl_order = Order {
                    id: uuid::Uuid::new_v4().to_string(),
                    exchange_id: None,
                    symbol: position.symbol.clone(),
                    order_type: OrderType::StopLoss,
                    side: stop_side,
                    price: Some(position.stop_loss),
                    amount: position.size,
                    filled_amount: 0.0,
                    status: OrderStatus::New,
                    created_at: Utc::now(),
                    updated_at: None,
                    position_id: Some(position.id.clone()),
                };
                
                let sl_result = self.create_order(new_sl_order).await?;
                updated_position.sl_order_id = sl_result.exchange_id;
            }
        }
        
        // If take profit changed, update the order
        if let Some(tp_id) = &position.tp_order_id {
            let tp_order = self.get_order(tp_id, &position.symbol).await?;
            
            // Check if we need to update the take profit
            let tp_price = tp_order.price.unwrap_or(0.0);
            if tp_price != position.take_profit {
                // Cancel existing take profit
                self.cancel_order(tp_id, &position.symbol).await?;
                
                // Create new take profit
                let close_side = self.close_position_side(&position.position_type);
                
                let new_tp_order = Order {
                    id: uuid::Uuid::new_v4().to_string(),
                    exchange_id: None,
                    symbol: position.symbol.clone(),
                    order_type: OrderType::TakeProfit,
                    side: close_side,
                    price: Some(position.take_profit),
                    amount: position.size,
                    filled_amount: 0.0,
                    status: OrderStatus::New,
                    created_at: Utc::now(),
                    updated_at: None,
                    position_id: Some(position.id.clone()),
                };
                
                let tp_result = self.create_order(new_tp_order).await?;
                updated_position.tp_order_id = tp_result.exchange_id;
            }
        }
        
        // Log the position update to InfluxDB
        if let Ok(signal) = self.convert_position_to_signal(&updated_position) {
            let _ = self.influx.write_signal(&signal).await;
        }
        
        Ok(updated_position)
    }
    
    fn convert_position_to_signal(&self, position: &Position) -> Result<crate::models::Signal> {
        Ok(crate::models::Signal {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: position.symbol.clone(),
            timestamp: Utc::now(),
            position_type: position.position_type.clone(),
            price: position.entry_price,
            reason: "Position update".to_string(),
            strength: 1.0,
            take_profit: position.take_profit,
            stop_loss: position.stop_loss,
            processed: true,
        })
    }