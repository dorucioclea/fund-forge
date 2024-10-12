use async_trait::async_trait;
use rust_decimal_macros::dec;
use ff_standard_lib::helpers::converters::fund_forge_formatted_symbol_name;
use ff_standard_lib::helpers::decimal_calculators::round_to_decimals;
use ff_standard_lib::messages::data_server_messaging::{DataServerResponse, FundForgeError};
use ff_standard_lib::server_features::server_side_brokerage::BrokerApiResponse;
use ff_standard_lib::standardized_types::broker_enum::Brokerage;
use ff_standard_lib::standardized_types::enums::StrategyMode;
use ff_standard_lib::standardized_types::new_types::Volume;
use ff_standard_lib::standardized_types::subscriptions::SymbolName;
use ff_standard_lib::standardized_types::symbol_info::SymbolInfo;
use ff_standard_lib::strategies::ledgers::{AccountId, AccountInfo, Currency};
use ff_standard_lib::StreamName;
use crate::test_api::api_client::TestApiClient;

#[async_trait]
impl BrokerApiResponse for TestApiClient {
    async fn symbol_names_response(&self, _mode: StrategyMode, _stream_name: StreamName,  callback_id: u64) -> DataServerResponse {
        DataServerResponse::SymbolNames {
            callback_id,
            symbol_names: vec![
                "EUR-USD".to_string(),
                "AUD-USD".to_string(),
                "AUD-CAD".to_string(),
            ],
        }
    }


    async fn account_info_response(&self, _mode: StrategyMode, _stream_name: StreamName, account_id: AccountId, callback_id: u64) -> DataServerResponse {
        let account_info = AccountInfo {
            brokerage: Brokerage::Test,
            cash_value: dec!(100000),
            cash_available:dec!(100000),
            currency: Currency::USD,
            cash_used: dec!(0),
            positions: vec![],
            account_id,
            is_hedging: false,
            buy_limit: None,
            sell_limit: None,
            max_orders: None,
            daily_max_loss: None,
            daily_max_loss_reset_time: None,
        };
        DataServerResponse::AccountInfo {
            callback_id,
            account_info,
        }
    }

    async fn symbol_info_response(
        &self,
        _mode: StrategyMode,
        _stream_name: StreamName,
        symbol_name: SymbolName,
        callback_id: u64
    ) -> DataServerResponse {
        let symbol_name = fund_forge_formatted_symbol_name(&symbol_name);
        let (pnl_currency, value_per_tick, tick_size) = match symbol_name.as_str() {
            "EUR-USD" => (Currency::USD, dec!(1.0), dec!(0.0001)), // EUR/USD with $1 per tick
            "AUD-CAD" => (Currency::USD, dec!(1.0), dec!(0.0001)), // AUD/CAD with $1 per tick (approximate)
            _ => (Currency::USD, dec!(0.1), dec!(0.00001))         // Default values
        };

        let symbol_info = SymbolInfo {
            symbol_name,
            pnl_currency,
            value_per_tick,
            tick_size,
            decimal_accuracy: 5,
        };

        DataServerResponse::SymbolInfo {
            callback_id,
            symbol_info,
        }
    }

    async fn margin_required_response(
        &self,
        _mode: StrategyMode,
        _stream_name: StreamName,
        symbol_name: SymbolName,
        quantity: Volume,
        callback_id: u64
    ) -> DataServerResponse {
        // Ensure quantity is not zero
        let symbol_name = fund_forge_formatted_symbol_name(&symbol_name);
        if quantity == dec!(0) {
            return DataServerResponse::Error {
                callback_id,
                error: FundForgeError::ClientSideErrorDebug("Quantity cannot be 0".to_string())
            };
        }

        // Assuming 100:1 leverage, calculate margin required
        // You may want to factor in symbol-specific prices if available
        let margin_required = round_to_decimals(quantity * dec!(100.0), 2);

        DataServerResponse::MarginRequired {
            callback_id,
            symbol_name,
            price: margin_required,  // Here price represents the margin required
        }
    }

    async fn accounts_response(&self, _mode: StrategyMode,_stream_name: StreamName, callback_id: u64) -> DataServerResponse {
       DataServerResponse::Accounts {callback_id, accounts: vec!["TestAccount1".to_string(), "TestAccount2".to_string()]}
    }

    async fn logout_command(&self, _stream_name: StreamName) {

    }
}