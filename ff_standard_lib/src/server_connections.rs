use std::collections::{HashMap};
use crate::servers::communications_async::{ExternalSender};
use crate::servers::init_clients::{create_async_api_client};
use crate::servers::settings::client_settings::{initialise_settings, ConnectionSettings};
use crate::standardized_types::data_server_messaging::{DataServerRequest, DataServerResponse, FundForgeError, StreamRequest};
use heck::ToPascalCase;
use lazy_static::lazy_static;
use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use ahash::AHashMap;
use async_std::task::sleep;
use chrono::{Duration, Utc};
use dashmap::DashMap;
use futures::SinkExt;
use strum_macros::Display;
use tokio::io;
use once_cell::sync::OnceCell;
use tokio::io::{AsyncReadExt, ReadHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::sync::mpsc::Sender;
use tokio_rustls::TlsStream;
use crate::apis::brokerage::broker_enum::Brokerage;
use crate::apis::data_vendor::datavendor_enum::DataVendor;
use crate::drawing_objects::drawing_object_handler::DrawingObjectHandler;
use crate::indicators::indicator_handler::{IndicatorHandler};
use crate::interaction_handler::InteractionHandler;
use crate::market_handler::market_handlers::MarketHandler;
use crate::standardized_types::enums::StrategyMode;
use crate::standardized_types::orders::orders::{OrderRequest};
use crate::standardized_types::strategy_events::{EventTimeSlice, StrategyEvent};
use crate::standardized_types::strategy_events::StrategyEvent::DataSubscriptionEvents;
use crate::standardized_types::subscription_handler::SubscriptionHandler;
use crate::standardized_types::subscriptions::{DataSubscription, DataSubscriptionEvent};
use crate::timed_events_handler::TimedEventHandler;
use crate::traits::bytes::Bytes;

pub const GUI_ENABLED: bool = true;
pub const GUI_DISABLED: bool = false;
const DEFAULT: ConnectionType = ConnectionType::Default;

/// A wrapper to allow us to pass in either a `Brokerage` or a `DataVendor`
/// # Variants
/// * `Broker(Brokerage)` - Containing a `Brokerage` object
/// * `Vendor(DataVendor)` - Containing a `DataVendor` object
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize, Debug, Display)]
pub enum ConnectionType {
    Vendor(DataVendor),
    Broker(Brokerage),
    Default,
    StrategyRegistry,
}

impl FromStr for ConnectionType {
    type Err = FundForgeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let string = s.to_pascal_case();
        match string.as_str() {
            "Default" => Ok(ConnectionType::Default),
            "StrategyRegistry" => Ok(ConnectionType::StrategyRegistry),
            _ if s.starts_with("Broker:") => {
                let data = s.trim_start_matches("Broker:").trim();
                Ok(ConnectionType::Broker(Brokerage::from_str(data)?))
            }
            _ if s.starts_with("Vendor:") => {
                let data = s.trim_start_matches("Vendor:").trim();
                Ok(ConnectionType::Vendor(DataVendor::from_str(data)?))
            }
            _ => Err(FundForgeError::ClientSideErrorDebug(format!(
                "Connection Type {} is not recognized",
                s
            ))),
        }
    }
}

/*
    1. primary data comes from either backtest engine or server stream
    2. primary data is broadcast to PRIMARY_DATA_BROADCASTER subscribers
        a. subscription handler
        b. indicator handler
    3. consolidated data is broadcast to subscribers
        a indicator handler

    4. each buffer iteration before sending the buffer to the engine or strategy, we update consolidator time.
        consolidator and indicator handler can return empty vec so we only add to slice if !is_empty(),
        The reason for returning empty buffer is so that we can block until the handlers have cycled the last data input,
        since we always expect Some to return. this allows async timings with concurrent data updates.

*/


// Connections
lazy_static! {
    static ref MAX_SERVERS: usize = initialise_settings().unwrap().len();
    static ref ASYNC_OUTGOING:  DashMap<ConnectionType, Arc<ExternalSender>> = DashMap::with_capacity(*MAX_SERVERS);
    static ref ASYNC_INCOMING: DashMap<ConnectionType, Arc<Mutex<ReadHalf<TlsStream<TcpStream>>>>> = DashMap::with_capacity(*MAX_SERVERS);
    static ref PRIMARY_SUBSCRIPTIONS: Arc<Mutex<Vec<DataSubscription>>> = Arc::new(Mutex::new(Vec::new()));
}

pub static SUBSCRIPTION_HANDLER: OnceCell<Arc<SubscriptionHandler>> = OnceCell::new();
pub async fn subscribe_primary_subscription_updates(name: String, sender: mpsc::Sender<Vec<DataSubscription>>) {
    *PRIMARY_SUBSCRIPTIONS.lock().await = SUBSCRIPTION_HANDLER.get().unwrap().primary_subscriptions().await;
    SUBSCRIPTION_HANDLER.get().unwrap().subscribe_primary_subscription_updates(name, sender).await // Return a clone of the Arc to avoid moving the value out of the OnceCell
}

pub static INDICATOR_HANDLER: OnceCell<Arc<IndicatorHandler>> = OnceCell::new();

pub static MARKET_HANDLER: OnceCell<Arc<MarketHandler>> = OnceCell::new();



static INTERACTION_HANDLER: OnceCell<Arc<InteractionHandler>> = OnceCell::new();
static TIMED_EVENT_HANDLER: OnceCell<Arc<TimedEventHandler>> = OnceCell::new();
static DRAWING_OBJECTS_HANDLER: OnceCell<Arc<DrawingObjectHandler>> = OnceCell::new();

pub async fn set_warmup_complete() {
    SUBSCRIPTION_HANDLER.get_or_init(|| {
        panic!("SUBSCRIPTION_HANDLER Not found")
    }).set_warmup_complete().await;
    INDICATOR_HANDLER.get_or_init(|| {
        panic!("INDICATOR_HANDLER Not found")
    }).set_warmup_complete().await;
    INTERACTION_HANDLER.get_or_init(|| {
        panic!("INTERACTION_HANDLER Not found")
    }).set_warmup_complete().await;
    TIMED_EVENT_HANDLER.get_or_init(|| {
        panic!("TIMED_EVENT_HANDLER Not found")
    }).set_warmup_complete().await;
}


pub(crate) enum StrategyRequest {
    CallBack(ConnectionType, DataServerRequest, oneshot::Sender<DataServerResponse>),
    OneWay(ConnectionType, DataServerRequest),
    Orders(ConnectionType, OrderRequest)
}
static DATA_SERVER_SENDER: OnceCell<Arc<Mutex<mpsc::Sender<StrategyRequest>>>> = OnceCell::new();
pub(crate) fn get_sender() -> Arc<Mutex<mpsc::Sender<StrategyRequest>>> {
    DATA_SERVER_SENDER.get().unwrap().clone() // Return a clone of the Arc to avoid moving the value out of the OnceCell
}
pub(crate) async fn send_request(req: StrategyRequest) {
    get_sender().lock().await.send(req).await.unwrap(); // Return a clone of the Arc to avoid moving the value out of the OnceCell
}

static STRATEGY_SENDER: OnceCell<Sender<EventTimeSlice>> = OnceCell::new();
pub async fn send_strategy_event_slice(slice: EventTimeSlice) {
    STRATEGY_SENDER.get().unwrap().send(slice).await.unwrap();
}

//Notes
//todo, make all handlers event driven.. we will need to use senders and receivers.
// 1. Subscriptions will be handled by the subscription handler, it will only send subscription request if it needs a new primary, it will alo cancel existing if need be.

pub async fn live_subscription_handler(mode: StrategyMode, subscription_update_channel: mpsc::Receiver<Vec<DataSubscription>>, settings_map: HashMap<ConnectionType, ConnectionSettings>) {
    if mode == StrategyMode::Backtest {
        return;
    }

    let mut subscription_update_channel = subscription_update_channel;
    let current_subscriptions = PRIMARY_SUBSCRIPTIONS.clone();
    println!("Handler: Start Live handler");
    tokio::task::spawn(async move {
        {
            let current_subscriptions = current_subscriptions.lock().await;
            println!("Handler: {:?}", current_subscriptions);
            for subscription in &*current_subscriptions {
                let request = DataServerRequest::StreamRequest {
                    request: StreamRequest::Subscribe(subscription.clone())
                };
                let connection = ConnectionType::Vendor(subscription.symbol.data_vendor.clone());
                let connection_type = match settings_map.contains_key(&connection) {
                    true => connection,
                    false => ConnectionType::Default
                };
                let register = StrategyRequest::OneWay(connection_type.clone(), DataServerRequest::Register(mode.clone()));
                send_request(register).await;
                let request = StrategyRequest::OneWay(connection_type, request);
                send_request(request).await;
            }
        }
        while let Some(updated_subscriptions) = subscription_update_channel.recv().await {
            let mut current_subscriptions = current_subscriptions.lock().await;
            let mut requests_map = AHashMap::new();
            if *current_subscriptions != updated_subscriptions {
                for subscription in &updated_subscriptions {
                    if !current_subscriptions.contains(&subscription) {
                        let connection = ConnectionType::Vendor(subscription.symbol.data_vendor.clone());
                        let connection_type = match settings_map.contains_key(&connection) {
                            true => connection,
                            false => ConnectionType::Default
                        };
                        let request = DataServerRequest::StreamRequest { request: StreamRequest::Subscribe(subscription.clone())};
                        if !requests_map.contains_key(&connection_type) {
                            requests_map.insert(connection_type, vec![request]);
                        } else {
                            requests_map.get_mut(&connection_type).unwrap().push(request);
                        }
                    }
                }
                for subscription in &*current_subscriptions {
                    if !updated_subscriptions.contains(&subscription) {
                        let connection = ConnectionType::Vendor(subscription.symbol.data_vendor.clone());
                        let connection_type = match settings_map.contains_key(&connection) {
                            true => connection,
                            false => ConnectionType::Default
                        };
                        let request = DataServerRequest::StreamRequest { request: StreamRequest::Unsubscribe(subscription.clone())};

                        if !requests_map.contains_key(&connection_type) {
                            requests_map.insert(connection_type, vec![request]);
                        } else {
                            requests_map.get_mut(&connection_type).unwrap().push(request);
                        }
                    }
                }
                for (connection, requests) in requests_map {
                    for request in requests {
                        let request = StrategyRequest::OneWay(connection.clone(), request);
                        send_request(request).await;
                    }
                }
                *current_subscriptions = updated_subscriptions.clone();
            }
        }
    });
}
/// This response handler is also acting as a live engine.
async fn request_handler(mode: StrategyMode, receiver: mpsc::Receiver<StrategyRequest>, buffer_duration: Option<Duration>, settings_map: HashMap<ConnectionType, ConnectionSettings>)  {
    let mut receiver = receiver;
    let callbacks: Arc<RwLock<AHashMap<u64, oneshot::Sender<DataServerResponse>>>> = Default::default();
    let connection_map = Arc::new(settings_map);
    let callbacks_ref = callbacks.clone();
    tokio::task::spawn(async move {
        let mut callback_id_counter: u64 = 0;
        let mut callbacks = callbacks_ref.clone();
        //println!("Request handler start");
        while let Some(outgoing_message) = receiver.recv().await {
            let connection_map = connection_map.clone();
            match outgoing_message {
                StrategyRequest::CallBack(connection_type, mut request, oneshot) => {
                    callback_id_counter += 1;
                    let callbacks = callbacks.clone();
                    let id = callback_id_counter.clone();
                    callbacks.write().await.insert(id, oneshot);
                    request.set_callback_id(id.clone());
                    tokio::task::spawn(async move {
                        let connection_type = match connection_map.contains_key(&connection_type) {
                            true => connection_type,
                            false => ConnectionType::Default
                        };
                        //println!("{}: request_received: {:?}", connection_type, request);
                        send(connection_type, request.to_bytes()).await;
                    });
                }
                StrategyRequest::OneWay(connection_type, mut request) => {
                    tokio::task::spawn(async move {
                        let connection_type = match connection_map.contains_key(&connection_type) {
                            true => connection_type,
                            false => ConnectionType::Default
                        };
                        //println!("{}: request_received: {:?}", connection_type, request);
                        send(connection_type, request.to_bytes()).await;
                    });
                }
                StrategyRequest::Orders(connection_type, request) => {
                    tokio::task::spawn(async move {
                        let brokerage = request.brokerage();
                        let request = DataServerRequest::OrderRequest {
                            request
                        };
                        let connection_type= ConnectionType::Broker(brokerage);
                        let connection_type = match connection_map.contains_key(&connection_type) {
                            true => connection_type,
                            false => ConnectionType::Default
                        };
                        //println!("{}: request_received: {:?}", connection_type, request);
                        send(connection_type, request.to_bytes()).await;
                    });
                }
            }
        }
        //println!("request handler end");
    });

    let callbacks = callbacks.clone();
    for incoming in ASYNC_INCOMING.iter() {
        let register_message = StrategyRequest::OneWay(incoming.key().clone(), DataServerRequest::Register(mode.clone()));
        send_request(register_message).await;
        let receiver = incoming.clone();
        let callbacks = callbacks.clone();
        tokio::task::spawn(async move {
            let mut receiver = receiver.lock().await;
            const LENGTH: usize = 8;
            //println!("{:?}: response handler start", incoming.key());
            let mut length_bytes = [0u8; LENGTH];
            while let Ok(_) = receiver.read_exact(&mut length_bytes).await {
                // Parse the length from the header
                let msg_length = u64::from_be_bytes(length_bytes) as usize;
                let mut message_body = vec![0u8; msg_length];

                // Read the message body based on the length
                match receiver.read_exact(&mut message_body).await {
                    Ok(_) => {
                        //eprintln!("Ok reading message body");
                    },
                    Err(e) => {
                        eprintln!("Error reading message body: {}", e);
                        continue;
                    }
                }
                // these will be buffered eventually into an EventTimeSlice
                let callbacks = callbacks.clone();
                tokio::task::spawn(async move {
                    let response = DataServerResponse::from_bytes(&message_body).unwrap();
                    match response.get_callback_id() {
                        // if there is no callback id then we add it to the strategy event buffer
                        None => {
                            match response {
                                DataServerResponse::SubscribeResponse { success, subscription, reason } => {
                                    //determine success or fail and add to the strategy event buffer
                                    match success {
                                        true => {
                                            let event = DataSubscriptionEvent::Subscribed(subscription.clone());
                                            let event_slice = StrategyEvent::DataSubscriptionEvents(vec![event], Utc::now().timestamp());
                                            send_strategy_event_slice(vec![event_slice]).await;
                                            println!("Handler: Subscribed: {}", subscription);
                                        }
                                        false => {
                                            let event = DataSubscriptionEvent::FailedSubscribed(subscription.clone(), reason.unwrap());
                                            let event_slice = StrategyEvent::DataSubscriptionEvents(vec![event], Utc::now().timestamp());
                                            send_strategy_event_slice(vec![event_slice]).await;
                                            println!("Handler: Subscribe Failed: {}", subscription);
                                        }
                                    }
                                }
                                DataServerResponse::UnSubscribeResponse { success, subscription, reason } => {
                                    match success {
                                        true => {
                                            let event = DataSubscriptionEvent::Unsubscribed(subscription);
                                            let event_slice = StrategyEvent::DataSubscriptionEvents(vec![event], Utc::now().timestamp());
                                            send_strategy_event_slice(vec![event_slice]).await;
                                        }
                                        false => {
                                            let event = DataSubscriptionEvent::FailedUnSubscribed(subscription, reason.unwrap());
                                            let event_slice = StrategyEvent::DataSubscriptionEvents(vec![event], Utc::now().timestamp());
                                            send_strategy_event_slice(vec![event_slice]).await;
                                        }
                                    }
                                }
                                DataServerResponse::DataUpdates(data) => {
                                    let event_slice = StrategyEvent::TimeSlice(Utc::now().to_string() ,data);
                                    send_strategy_event_slice(vec![event_slice]).await;
                                }
                                _ => panic!("Incorrect response here: {:?}", response)
                            }
                        }
                        // if there is a callback id we just send it to the awaiting oneshot receiver
                        Some(id) => {
                            if let Some(callback) = callbacks.write().await.remove(&id) {
                                let _ = callback.send(response);
                            }
                        }
                    }
                });
            }
        });
    }
}

pub async fn send(connection_type: ConnectionType, msg: Vec<u8>) {
    let sender = ASYNC_OUTGOING.get(&connection_type).unwrap_or_else(|| ASYNC_OUTGOING.get(&ConnectionType::Default).unwrap()).value().clone();
    match sender.send(&msg).await {
        Ok(_) => {}
        Err(e) => eprintln!("{}", e)
    }
}

pub async fn init_sub_handler(subscription_handler: Arc<SubscriptionHandler>) {
    SUBSCRIPTION_HANDLER.get_or_init(|| {
        subscription_handler
    }).clone();
}
pub async fn initialize_static(
    mode: StrategyMode,
    event_sender: mpsc::Sender<EventTimeSlice>,
    indicator_handler: Arc<IndicatorHandler>,
    market_handler: Arc<MarketHandler>,
    timed_event_handler: Arc<TimedEventHandler>,
    interaction_handler: Arc<InteractionHandler>,
    drawing_objects_handler: Arc<DrawingObjectHandler>,
) {

    STRATEGY_SENDER.get_or_init(|| {
        event_sender
    }).clone();
    INDICATOR_HANDLER.get_or_init(|| {
        indicator_handler
    }).clone();
    TIMED_EVENT_HANDLER.get_or_init(|| {
        timed_event_handler
    }).clone();
    MARKET_HANDLER.get_or_init(|| {
        market_handler
    }).clone();
    INTERACTION_HANDLER.get_or_init(|| {
        interaction_handler
    }).clone();
    DRAWING_OBJECTS_HANDLER.get_or_init(|| {
        drawing_objects_handler
    }).clone();
}

pub async fn init_connections(gui_enabled: bool, buffer_duration: Option<Duration>, mode: StrategyMode) {
    //initialize_strategy_registry(platform_mode.clone()).await;
    let settings_map = initialise_settings().unwrap();
    println!("Connections: {:?}", settings_map);
    // for each connection type specified in our server_settings.toml we will establish a connection
    for (connection_type, settings) in settings_map.iter() {
        if !gui_enabled && connection_type == &ConnectionType::StrategyRegistry {
            continue
        }
        // set up async client
        let async_client = match create_async_api_client(&settings).await {
            Ok(client) => client,
            Err(__e) => { eprintln!("{}", format!("Unable to establish connection to: {:?} server @ address: {:?}", connection_type, settings));
                continue;
            }
        };
        let (read_half, write_half) = io::split(async_client);
        let async_sender = ExternalSender::new(write_half);
        ASYNC_OUTGOING.insert(connection_type.clone(), Arc::new(async_sender));
        ASYNC_INCOMING.insert(connection_type.clone(), Arc::new(Mutex::new(read_half)));
    }
    let (tx, rx) = mpsc::channel(1000);
    DATA_SERVER_SENDER.get_or_init(|| {
        Arc::new(Mutex::new(tx))
    }).clone();

    request_handler(mode.clone(), rx, buffer_duration, settings_map.clone()).await;
}
