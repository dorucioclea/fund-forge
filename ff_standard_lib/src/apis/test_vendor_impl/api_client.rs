use crate::apis::brokerage::server_responses::BrokerApiResponse;
use crate::apis::brokerage::Brokerage;
use crate::apis::vendor::server_responses::VendorApiResponse;
use crate::apis::vendor::DataVendor;
use crate::standardized_types::accounts::ledgers::{AccountCurrency, AccountId, AccountInfo, SymbolInfo};
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::data_server_messaging::{FundForgeError, SynchronousResponseType};
use crate::standardized_types::enums::{MarketType, Resolution, SubscriptionResolutionType};
use crate::standardized_types::subscriptions::{Symbol, SymbolName};
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;

static TEST_API_CLIENT: OnceCell<Arc<TestVendorApi>> = OnceCell::new();

pub async fn get_test_api_client() -> Arc<TestVendorApi> {
    TEST_API_CLIENT
        .get_or_init(|| Arc::new(TestVendorApi::new()))
        .clone()
}

#[derive(
    Serialize,
    Deserialize,
    Clone,
    Eq,
    Serialize_rkyv,
    Deserialize_rkyv,
    Archive,
    PartialEq,
    Debug,
    Hash,
    PartialOrd,
    Ord,
)]
#[archive(
// This will generate a PartialEq impl between our unarchived and archived
// types:
compare(PartialEq),
// bytecheck can be used to validate your data if you want. To use the safe
// API, you have to derive CheckBytes for the archived type:
check_bytes,
)]
#[archive_attr(derive(Debug))]
/// A Test Data vendor to guide development
/// This would represent a list of functions that link an api implementation to the common functions required by fund forge to interact with a data vendor.
pub struct TestVendorApi {}

impl TestVendorApi {
    pub fn new() -> TestVendorApi {
        TestVendorApi {}
    }
}

#[async_trait]
impl VendorApiResponse for TestVendorApi {
    async fn basedata_symbols_response(
        &self,
        market_type: MarketType,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        let mut symbols = Vec::new();
        if market_type != MarketType::Forex {
            return Err(FundForgeError::ClientSideErrorDebug(
                "Market Type not supported".to_string(),
            ));
        }
        // we retrieve the list of symbols from the vendor (in this case we just have 2 hardcoded symbols to return, but this would be a method call to the vendor api)
        let symbol = Symbol::new("AUD-USD".to_string(), DataVendor::Test, MarketType::Forex);
        symbols.push(symbol);
        let symbol2 = Symbol::new("AUD-CAD".to_string(), DataVendor::Test, MarketType::Forex);
        symbols.push(symbol2);

        // we serialize the symbols into bytes for transport
        Ok(SynchronousResponseType::Symbols(symbols, market_type))
    }

    async fn resolutions_response(
        &self,
        market_type: MarketType,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        if market_type != MarketType::Forex {
            return Err(FundForgeError::ClientSideErrorDebug(
                "Market Type not supported".to_string(),
            ));
        }
        let data_resolution =
            SubscriptionResolutionType::new(Resolution::Instant, BaseDataType::Quotes);
        Ok(SynchronousResponseType::Resolutions(
            vec![data_resolution],
            market_type,
        ))
    }

    async fn markets_response(&self) -> Result<SynchronousResponseType, FundForgeError> {
        Ok(SynchronousResponseType::Markets(vec![MarketType::Forex]))
    }

    async fn decimal_accuracy_response(
        &self,
        _symbol_name: SymbolName,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        Ok(SynchronousResponseType::DecimalAccuracy(5))
    }

    async fn tick_size_response(
        &self,
        symbol_name: SymbolName,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        Ok(SynchronousResponseType::TickSize(Decimal::from_f64(0.00001).unwrap()))
    }
}

#[async_trait]
impl BrokerApiResponse for TestVendorApi {
    async fn symbols_response(
        &self,
        market_type: MarketType,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        self.basedata_symbols_response(market_type).await
    }

    async fn account_currency_response(
        &self,
        account_id: AccountId,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        let response = SynchronousResponseType::AccountCurrency(account_id, AccountCurrency::USD);
        Ok(response)
    }

    async fn account_info_response(
        &self,
        account_id: AccountId,
    ) -> Result<SynchronousResponseType, FundForgeError> {
        let info = AccountInfo {
            account_id: account_id.clone(),
            brokerage: Brokerage::Test,
            cash_value: dec!(100000.0),
            cash_available: dec!(100000.0),
            currency: AccountCurrency::USD,
            cash_used: dec!(0.0),
            positions: Default::default(),
            positions_closed: Default::default(),
            is_hedging: true,
        };
        let response = SynchronousResponseType::AccountInfo(info);
        Ok(response)
    }

    async fn symbol_info_response(&self, symbol_name: SymbolName) -> Result<SynchronousResponseType, FundForgeError> {
        match symbol_name {
            _ => Ok(SynchronousResponseType::SymbolInfo(SymbolInfo::new(symbol_name, AccountCurrency::USD, Decimal::from_f64(0.00001).unwrap(), Decimal::from_f64(0.00001).unwrap())))
        }
    }
}
