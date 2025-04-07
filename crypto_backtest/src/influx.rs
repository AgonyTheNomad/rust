use crate::models::Candle;
use influxdb2::{Client, models::Query};
use influxdb2_structmap::value::Value; // Use the correct Value type
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::error::Error;

pub async fn get_candles(
    url: &str,
    token: &str,
    org: &str,
    bucket: &str,
) -> Result<Vec<Candle>, Box<dyn Error>> {
    let client = Client::new(url, token, org);

    // Construct the Flux query
    let query = Query::new(format!(
        r#"
        from(bucket: "{}")
        |> range(start: -1d)
        |> filter(fn: (r) => r._measurement == "candles" and r.symbol == "BTC")
        |> pivot(rowKey:["_time"], columnKey: ["_field"], valueColumn: "_value")
        "#,
        bucket
    ));

    // Execute the query
    let raw_result = client.query_raw(Some(query)).await?;

    // Parse the raw results into Candles
    let mut candles = Vec::new();
    for record in raw_result {
        let values: &BTreeMap<String, Value> = &record.values;

        let time = values
            .get("_time")
            .and_then(|v| v.as_str())
            .ok_or("Missing _time field")?
            .parse::<DateTime<Utc>>()?;

        let candle = Candle {
            time,
            open: values.get("open").and_then(|v| v.as_f64()).unwrap_or(0.0),
            high: values.get("high").and_then(|v| v.as_f64()).unwrap_or(0.0),
            low: values.get("low").and_then(|v| v.as_f64()).unwrap_or(0.0),
            close: values.get("close").and_then(|v| v.as_f64()).unwrap_or(0.0),
            volume: values.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0),
            num_trades: values.get("num_trades").and_then(|v| v.as_i64()).unwrap_or(0),
        };

        candles.push(candle);
    }

    Ok(candles)
}
