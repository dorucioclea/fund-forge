use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tokio::sync::oneshot;
use crate::messages::data_server_messaging::{DataServerRequest, DataServerResponse, FundForgeError};
use crate::product_maps::oanda::maps::{OANDA_FX_SYMBOLS};
use crate::standardized_types::accounts::Currency;
use crate::standardized_types::datavendor_enum::DataVendor;
use crate::standardized_types::enums::OrderSide;
use crate::strategies::client_features::connection_types::ConnectionType;
use crate::strategies::client_features::request_handler::{send_request, StrategyRequest};

pub async fn get_exchange_rate(from_currency: Currency, to_currency: Currency, date_time: DateTime<Utc>, side: OrderSide) -> Result<Decimal, FundForgeError> {
    let currency_pair_string = format!("{}-{}", from_currency.to_string(), to_currency.to_string());
    let data_vendor = match OANDA_FX_SYMBOLS.contains(&currency_pair_string) {
        true => DataVendor::Oanda,
        false => {
            let currency_pair_string = format!("{}-{}", to_currency.to_string(), from_currency.to_string());
            match OANDA_FX_SYMBOLS.contains(&currency_pair_string) {
                true => DataVendor::Oanda,
                false => DataVendor::Bitget
            }
        }
    };
    let request = DataServerRequest::ExchangeRate {
        callback_id: 0,
        from_currency,
        to_currency,
        date_time_string: date_time.to_string(),
        data_vendor,
        side
    };
    //eprintln!("Getting exchange rate for {}-{} at {}", from_currency.to_string(), to_currency.to_string(), date_time);
    let (sender, receiver) = oneshot::channel();
    let msg = StrategyRequest::CallBack(ConnectionType::Vendor(data_vendor), request, sender);
    send_request(msg).await;
    match receiver.await {
        Ok(response) => match response {
            DataServerResponse::ExchangeRate { rate, .. } => Ok(rate),
            DataServerResponse::Error { error, .. } => Err(error),
            _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
        },
        Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
    }
}