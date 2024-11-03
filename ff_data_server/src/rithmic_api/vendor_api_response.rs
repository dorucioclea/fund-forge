use std::cmp::min;
use std::collections::BTreeMap;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Utc};
use indicatif::{ProgressBar, ProgressStyle};
use crate::rithmic_api::client_base::rithmic_proto_objects::rti::request_login::SysInfraType;
use crate::rithmic_api::client_base::rithmic_proto_objects::rti::{RequestMarketDataUpdate, RequestTimeBarUpdate};
use crate::rithmic_api::client_base::rithmic_proto_objects::rti::request_time_bar_update::BarType;
use ff_standard_lib::messages::data_server_messaging::{DataServerResponse, FundForgeError};
use crate::server_features::server_side_datavendor::VendorApiResponse;
use ff_standard_lib::standardized_types::base_data::base_data_type::BaseDataType;
use ff_standard_lib::standardized_types::enums::{FuturesExchange, MarketType, StrategyMode, SubscriptionResolutionType};
use ff_standard_lib::standardized_types::resolution::Resolution;
use ff_standard_lib::standardized_types::subscriptions::{DataSubscription, Symbol, SymbolName};
use ff_standard_lib::StreamName;
use tokio::sync::{broadcast, oneshot};
use tokio::time::{timeout};
use ff_standard_lib::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::rithmic_api::api_client::{RithmicBrokerageClient, RITHMIC_DATA_IS_CONNECTED};
use crate::rithmic_api::products::{get_available_symbol_names, get_exchange_by_symbol_name, get_symbol_info};
use crate::server_features::database::{DATA_STORAGE};
use crate::stream_tasks::{subscribe_stream, unsubscribe_stream};

#[allow(dead_code)]
#[async_trait]
impl VendorApiResponse for RithmicBrokerageClient {
    async fn symbols_response(&self, _mode: StrategyMode, _stream_name: StreamName, market_type: MarketType, _time: Option<DateTime<Utc>>, callback_id: u64) -> DataServerResponse{
        let names = get_available_symbol_names();
        let mut symbols = Vec::new();
        for name in names {
            let exchange = match get_exchange_by_symbol_name(name) {
                Some(exchange) => exchange,
                None => continue
            };
            symbols.push(Symbol::new(name.clone(), self.data_vendor.clone(), MarketType::Futures(exchange)));
        }
        DataServerResponse::Symbols {
            callback_id,
            symbols,
            market_type,
        }
       /* match mode {
            StrategyMode::Backtest => {

            }
            StrategyMode::LivePaperTrading | StrategyMode::Live => {
                match market_type {
                    //todo, use this in a rithmic only fn, to get the toi products, just return the hardcoded list here.
                    MarketType::Futures(exchange) => {
                        let _req = RequestProductCodes {
                            template_id: 111 ,
                            user_msg: vec![stream_name.to_string(), callback_id.to_string()],
                            exchange: Some(exchange.to_string()),
                            give_toi_products_only: Some(true),
                        };
                    }
                    _ => return DataServerResponse::Error {callback_id, error: FundForgeError::ClientSideErrorDebug(format!("Incrorrect market type: {} for: {}", market_type, self.data_vendor))}
                }
            }
        }*/
    }

    async fn resolutions_response(&self, mode: StrategyMode, _stream_name: StreamName, _market_type: MarketType, callback_id: u64) -> DataServerResponse {
        let mut resolutions = Vec::new();
        match mode {
            StrategyMode::Backtest => {
                //todo, we need a better way to handle historical, primary data sources, we need a way to check for each symbol, which historical data is available.
                // to achieve this this fn should be split, resolutions should also be determined by symbol name when historical data is requested, so we can check the data we actually have available.
                resolutions.push(SubscriptionResolutionType::new(Resolution::Ticks(1), BaseDataType::Ticks));
            }
            StrategyMode::LivePaperTrading |  StrategyMode::Live => {
                resolutions.push(SubscriptionResolutionType::new(Resolution::Ticks(1), BaseDataType::Ticks));
                resolutions.push(SubscriptionResolutionType::new(Resolution::Instant, BaseDataType::Quotes));
                resolutions.push(SubscriptionResolutionType::new(Resolution::Seconds(1), BaseDataType::Candles));
            }
        }

        DataServerResponse::Resolutions {
            callback_id,
            subscription_resolutions_types: resolutions,
            market_type: MarketType::Forex,
        }
    }

    async fn markets_response(&self, _mode: StrategyMode, _stream_name: StreamName, callback_id: u64) -> DataServerResponse {
        DataServerResponse::Markets {
            callback_id,
            markets: vec![
                MarketType::Futures(FuturesExchange::CME),
                MarketType::Futures(FuturesExchange::CBOT),
                MarketType::Futures(FuturesExchange::COMEX),
                MarketType::Futures(FuturesExchange::NYBOT),
                MarketType::Futures(FuturesExchange::NYMEX),
                MarketType::Futures(FuturesExchange::MGEX)
            ],
        }
    }

    async fn decimal_accuracy_response(&self, _mode: StrategyMode, _stream_name: StreamName, symbol_name: SymbolName, callback_id: u64) -> DataServerResponse {
        let info = match get_symbol_info(&symbol_name) {
            Ok(info) => {
                info
            }
            Err(_e) => {
                return DataServerResponse::Error {callback_id, error: FundForgeError::ClientSideErrorDebug(format!("{} Accuracy Info not found with: {}", symbol_name, self.data_vendor))}
            }
        };
        DataServerResponse::DecimalAccuracy {
            callback_id,
            accuracy: info.decimal_accuracy,
        }
    }

    async fn tick_size_response(&self, _mode: StrategyMode, _stream_name: StreamName, symbol_name: SymbolName, callback_id: u64) -> DataServerResponse {
        let info = match get_symbol_info(&symbol_name) {
            Ok(info) => {
                info
            }
            Err(_e) => {
                return DataServerResponse::Error {callback_id, error: FundForgeError::ClientSideErrorDebug(format!("{} Tick Size Info not found with: {}", symbol_name, self.data_vendor))}
            }
        };
        DataServerResponse::TickSize {
            callback_id,
            tick_size: info.tick_size,
        }
    }

    async fn data_feed_subscribe(&self, stream_name: StreamName, subscription: DataSubscription) -> DataServerResponse {
        if !RITHMIC_DATA_IS_CONNECTED.load(std::sync::atomic::Ordering::SeqCst) {
            return DataServerResponse::SubscribeResponse{ success: false, subscription: subscription.clone(), reason: Some(format!("{} is not connected", self.data_vendor))}
        }

        let exchange = match subscription.market_type {
            MarketType::Futures(exchange) => {
                exchange.to_string()
            }
            _ => todo!()
        };

        let symbols = get_available_symbol_names();
        if !symbols.contains(&subscription.symbol.name) {
            return DataServerResponse::SubscribeResponse{ success: false, subscription: subscription.clone(), reason: Some(format!("This subscription is not available with {}: {}", subscription.symbol.data_vendor, subscription))}
        }

        let mut resolutions = Vec::new();
        resolutions.push(Resolution::Instant);
        resolutions.push(Resolution::Ticks(1));
        resolutions.push(Resolution::Seconds(1));
        //we can pass in live here because backtest never calls this fn

        if !resolutions.contains(&subscription.resolution) {
            return DataServerResponse::SubscribeResponse{ success: false, subscription: subscription.clone(), reason: Some(format!("This subscription is not available with {}: {}", subscription.symbol.data_vendor, subscription))}
        }

        const BASEDATA_TYPES: &[BaseDataType] = &[BaseDataType::Ticks, BaseDataType::Quotes, BaseDataType::Candles];
        if !BASEDATA_TYPES.contains(&subscription.base_data_type) {
            return DataServerResponse::SubscribeResponse{ success: false, subscription: subscription.clone(), reason: Some(format!("This subscription is not available with {}: {}", subscription.symbol.data_vendor, subscription))}
        };

        let mut is_subscribed = true;
        //todo have a unique function per base data type.
        match subscription.base_data_type {
            BaseDataType::Ticks => {
                if let Some(broadcaster) = self.tick_feed_broadcasters.get(&subscription.symbol.name) {
                    let receiver = broadcaster.value().subscribe();
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                } else {
                    let (sender, receiver) = broadcast::channel(500);
                    self.tick_feed_broadcasters.insert(subscription.symbol.name.clone(), sender);
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                    is_subscribed = false;
                }
            }
            BaseDataType::Quotes => {
                if let Some(broadcaster) = self.quote_feed_broadcasters.get(&subscription.symbol.name) {
                    let receiver = broadcaster.value().subscribe();
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                } else {
                    let (sender, receiver) = broadcast::channel(500);
                    self.quote_feed_broadcasters.insert(subscription.symbol.name.clone(), sender);
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                    self.ask_book.insert(subscription.symbol.name.clone(), BTreeMap::new());
                    self.ask_book.insert(subscription.symbol.name.clone(), BTreeMap::new());
                    is_subscribed = false;
                }
            }
            BaseDataType::Candles => {
                if let Some(broadcaster) = self.candle_feed_broadcasters.get(&subscription.symbol.name) {
                    let receiver = broadcaster.value().subscribe();
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                } else {
                    let (sender, receiver) = broadcast::channel(500);
                    self.candle_feed_broadcasters.insert(subscription.symbol.name.clone(), sender);
                    subscribe_stream(&stream_name, subscription.clone(), receiver).await;
                    is_subscribed = false;
                }
            }
            _ => todo!("Handle gracefully by returning err")
        }

        if !is_subscribed {
            if subscription.base_data_type == BaseDataType::Quotes || subscription.base_data_type == BaseDataType::Ticks {
                let bits = match subscription.base_data_type {
                    BaseDataType::Ticks => 1,
                    BaseDataType::Quotes => 2,
                    _ => return DataServerResponse::SubscribeResponse { success: false, subscription: subscription.clone(), reason: Some(format!("This subscription is not available with {}: {}", self.data_vendor, subscription)) }
                    //BaseDataType::Candles => {}
                };
                let req = RequestMarketDataUpdate {
                    template_id: 100,
                    user_msg: vec![],
                    symbol: Some(subscription.symbol.name.to_string()),
                    exchange: Some(exchange),
                    request: Some(1), //1 subscribe 2 unsubscribe
                    update_bits: Some(bits), //1 for ticks 2 for quotes
                };

                const PLANT: SysInfraType = SysInfraType::TickerPlant;
                self.send_message(&PLANT, req).await;
            } else if subscription.base_data_type == BaseDataType::Candles {
                let (num, res_type) = match subscription.resolution {
                    Resolution::Seconds(num) => (num as i32, BarType::SecondBar),
                    Resolution::Minutes(num) => (num as i32, BarType::MinuteBar),
                    _ => return DataServerResponse::SubscribeResponse { success: false, subscription: subscription.clone(), reason: Some(format!("This subscription is not available with {}: {}", self.data_vendor,subscription)) }
                };
                let req =RequestTimeBarUpdate {
                    template_id: 200,
                    user_msg: vec![],
                    symbol: Some(subscription.symbol.name.to_string()),
                    exchange: Some(exchange),
                    request: Some(1), //1 subscribe 2 unsubscribe
                    bar_type: Some(res_type.into()),
                    bar_type_period: Some(num),
                };
                const PLANT: SysInfraType = SysInfraType::HistoryPlant;
                self.send_message(&PLANT, req).await;
            }
        }
        println!("{} Subscribed: {}", stream_name, subscription);
        DataServerResponse::SubscribeResponse{ success: true, subscription: subscription.clone(), reason: None}
    }

    async fn data_feed_unsubscribe(&self, _mode: StrategyMode, stream_name: StreamName, subscription: DataSubscription) -> DataServerResponse {
        let exchange = match subscription.market_type {
            MarketType::Futures(exchange) => exchange.to_string(),
            _ => return DataServerResponse::UnSubscribeResponse {
                success: false,
                subscription: subscription.clone(),
                reason: Some(format!("Unsupported market type: {:?}", subscription.market_type)),
            },
        };

        unsubscribe_stream(&stream_name, &subscription).await;

        let (bits, broadcaster_map) = match subscription.base_data_type {
            BaseDataType::Ticks => (1, &self.tick_feed_broadcasters),
            BaseDataType::Quotes => (2, &self.quote_feed_broadcasters),
            BaseDataType::Candles => (3, &self.candle_feed_broadcasters),
            _ => return DataServerResponse::SubscribeResponse {
                success: false,
                subscription: subscription.clone(),
                reason: Some(format!("Unsupported data type: {:?}", subscription.base_data_type)),
            },
        };

        let symbol = subscription.symbol.name.clone();
        let mut should_disconnect = false;

        if let Some(broadcaster) = broadcaster_map.get_mut(&symbol) {
            should_disconnect = broadcaster.receiver_count() == 0;
        }

        if should_disconnect {
            if subscription.base_data_type == BaseDataType::Quotes || subscription.base_data_type == BaseDataType::Ticks {
                broadcaster_map.remove(&symbol);

                let req = RequestMarketDataUpdate {
                    template_id: 100,
                    user_msg: vec![],
                    symbol: Some(symbol.clone()),
                    exchange: Some(exchange),
                    request: Some(2), // 2 for unsubscribe
                    update_bits: Some(bits),
                };

                const PLANT: SysInfraType = SysInfraType::TickerPlant;
                self.send_message(&PLANT, req).await;

                // Additional cleanup for quotes
                if subscription.base_data_type == BaseDataType::Quotes {
                    self.ask_book.remove(&symbol);
                    self.bid_book.remove(&symbol);
                }
            } else if subscription.base_data_type == BaseDataType::Candles {
                let req =RequestTimeBarUpdate {
                    template_id: 200,
                    user_msg: vec![],
                    symbol: Some(subscription.symbol.name.to_string()),
                    exchange: Some(exchange),
                    request: Some(2), //1 subscribe 2 unsubscribe
                    bar_type: Some(BarType::SecondBar.into()),
                    bar_type_period: Some(1),
                };
                const PLANT: SysInfraType = SysInfraType::HistoryPlant;
                self.send_message(&PLANT, req).await;
            }
        }

        // Check if we need to switch heartbeat
        if self.tick_feed_broadcasters.is_empty() &&
            self.quote_feed_broadcasters.is_empty() &&
            self.candle_feed_broadcasters.is_empty()
        {
            //todo fix in ff_rithmic api this causes a lock
         /*   if let Err(e) = self.client.switch_heartbeat_required(SysInfraType::TickerPlant, true).await {
                eprintln!("Failed to switch heartbeat: {}", e);
            }*/
        }

        DataServerResponse::UnSubscribeResponse {
            success: true,
            subscription,
            reason: None,
        }
    }

    async fn base_data_types_response(&self, _mode: StrategyMode, _stream_name: StreamName, callback_id: u64) -> DataServerResponse {
        //todo get dynamically from server using stream name to fwd callback
        DataServerResponse::BaseDataTypes {
            callback_id,
            base_data_types: vec![BaseDataType::Ticks, BaseDataType::Quotes, BaseDataType::Candles],
        }
    }

    async fn logout_command_vendors(&self, stream_name: StreamName) {
        self.callbacks.remove(&stream_name);
    }

    #[allow(unused)]
    async fn session_market_hours_response(&self, mode: StrategyMode, stream_name: StreamName, symbol_name: SymbolName, date_time: DateTime<Utc>, callback_id: u64) -> DataServerResponse {
        todo!()
    }

    async fn update_historical_data(&self, symbol: Symbol, base_data_type: BaseDataType, resolution: Resolution, from: DateTime<Utc>, to: DateTime<Utc>, from_back: bool, progress_bar: ProgressBar) -> Result<(), FundForgeError> {
        const SYSTEM: SysInfraType = SysInfraType::HistoryPlant;
        const TIME_NEGATIVE: std::time::Duration = std::time::Duration::from_secs(1);
        let symbol_name = symbol.name.clone();
        let exchange = match get_exchange_by_symbol_name(&symbol_name) {
            Some(exchange) => exchange,
            None => {
                progress_bar.finish_and_clear();
                return Err(FundForgeError::ClientSideErrorDebug(format!("Exchange not found for symbol: {}", symbol_name)))
            }
        };

        let data_storage = DATA_STORAGE.get().unwrap();
        let mut window_start = from;
        let current_to = match from_back {
            true => to,
            false => Utc::now(),
        };

        let total_seconds = (current_to - from).num_seconds().abs();
        let bar_len = match resolution {
            Resolution::Ticks(_) => (total_seconds / (4 * 3600)) as u64 + 1,  // 4-hour chunks
            Resolution::Seconds(interval) => ((total_seconds / interval as i64) / 3600) as u64 + 1,  // hourly chunks adjusted by interval
            Resolution::Minutes(interval) => ((total_seconds / (interval as i64 * 60)) / (24 * 3600)) as u64 + 1,  // daily chunks adjusted by interval
            Resolution::Hours(interval) => ((total_seconds / (interval as i64 * 3600)) / (7 * 24 * 3600)) as u64 + 1,  // weekly chunks adjusted by interval
            _ => (total_seconds / (4 * 3600)) as u64 + 1,  // default to tick chunks
        };

        progress_bar.set_length(bar_len);
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("{prefix:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} ({eta})")
                .unwrap()
                .progress_chars("=>-")
        );

        let mut save_attempts = 0;
        let mut empty_windows = 0;
        'main_loop: loop {
            if window_start >= current_to - TIME_NEGATIVE {
                break 'main_loop;
            }

            let window_end = window_start + min(Duration::hours(4), current_to - window_start);

            progress_bar.set_message(format!("Downloading: ({}: {}) from: {}, to {}",
                                             resolution, base_data_type, from, current_to.format("%Y-%m-%d %H:%M:%S")));

            let (sender, receiver) = oneshot::channel();

            let permit = match self.historical_permits.acquire().await {
                Ok(permit) => permit,
                Err(_) => return Err(FundForgeError::ClientSideErrorDebug("Failed to acquire semaphore permit".to_string()))
            };

            self.send_replay_request(base_data_type, resolution, symbol_name.clone(), exchange, window_start, window_end, sender).await;
            const TIME_OUT: std::time::Duration = std::time::Duration::from_secs(180);
            let data_map = match timeout(TIME_OUT, receiver).await {
                Ok(receiver_result) => match receiver_result {
                    Ok(response) => {
                        if response.is_empty() {
                            empty_windows += 1;
                            if empty_windows > 200 {
                                break 'main_loop;
                            }
                        } else {
                            empty_windows = 0;
                        }
                        response
                    },
                    Err(_) => break 'main_loop
                },
                Err(_) => break 'main_loop
            };
            drop(permit);

            let mut is_saving = false;
            let mut is_end = false;
            let back_up_time = window_start.clone();
            if let Some((&last_time, _)) = data_map.last_key_value() {
                if last_time.day() != window_start.day() {
                    is_saving = true;
                }
                if last_time > window_start {
                    window_start = last_time;
                } else {
                    window_start = window_end;
                }

                if last_time >= current_to - TIME_NEGATIVE {
                    is_end = true;
                }
            } else {
                window_start = window_end;
                if window_start >= current_to - TIME_NEGATIVE {
                    is_end = true;
                }
            };

            if is_saving {
                let save_data: Vec<BaseDataEnum> = data_map.clone().into_values().collect();
                if let Err(_e) = data_storage.save_data_bulk(save_data).await {
                    window_start = back_up_time;
                    if save_attempts < 3 {
                        save_attempts += 1;
                        continue 'main_loop;
                    }
                }
                save_attempts = 0;
            }

            if is_end {
                break 'main_loop;
            }
            progress_bar.inc(1);
        }
        progress_bar.finish_and_clear();
        Ok(())
    }
}
