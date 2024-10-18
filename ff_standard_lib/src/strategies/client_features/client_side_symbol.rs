use tokio::sync::oneshot;
use tokio::time::timeout;
use crate::messages::data_server_messaging::{DataServerRequest, DataServerResponse, FundForgeError};
use crate::standardized_types::new_types::Price;
use crate::standardized_types::subscriptions::{Symbol, SymbolName};
use crate::strategies::client_features::client_side_brokerage::TIME_OUT;
use crate::strategies::client_features::connection_types::ConnectionType;
use crate::strategies::client_features::server_connections::{send_request, StrategyRequest};

impl Symbol {
    pub async fn tick_size(&self) -> Result<Price, FundForgeError> {
        let request = DataServerRequest::TickSize {
            callback_id: 0,
            data_vendor: self.data_vendor.clone(),
            symbol_name: self.name.clone(),
        };
        let (sender, receiver) = oneshot::channel();
        let msg = StrategyRequest::CallBack(ConnectionType::Vendor(self.data_vendor.clone()), request, sender);
        send_request(msg).await;
        match timeout(TIME_OUT, receiver).await {
            Ok(receiver_result) => match receiver_result {
                Ok(response) => {
                    match response {
                        DataServerResponse::TickSize { tick_size, .. } => Ok(tick_size),
                        DataServerResponse::Error { error, .. } => Err(error),
                        _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
                    }
                },
                Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
            },
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Operation timed out after {} seconds", e)))
        }
    }

    pub async fn decimal_accuracy(&self, symbol_name: SymbolName) -> Result<u32, FundForgeError> {
        let request = DataServerRequest::DecimalAccuracy {
            callback_id: 0,
            data_vendor: self.data_vendor.clone(),
            symbol_name,
        };
        let (sender, receiver) = oneshot::channel();
        let msg = StrategyRequest::CallBack(ConnectionType::Vendor(self.data_vendor.clone()), request,sender);
        send_request(msg).await;
        match timeout(TIME_OUT, receiver).await {
            Ok(receiver_result) => match receiver_result {
                Ok(response) => {
                    match response {
                        DataServerResponse::DecimalAccuracy { accuracy, .. } => Ok(accuracy),
                        DataServerResponse::Error {error,..} => Err(error),
                        _ => Err(FundForgeError::ClientSideErrorDebug("Incorrect response received at callback".to_string()))
                    }
                },
                Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Receiver error at callback recv: {}", e)))
            },
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(format!("Operation timed out after {} seconds", e)))
        }
    }
}