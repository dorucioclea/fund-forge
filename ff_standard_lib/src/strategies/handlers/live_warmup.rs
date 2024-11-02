use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Datelike, NaiveTime, TimeZone, Utc};
use tokio::sync::mpsc::Sender;
use crate::standardized_types::base_data::history::get_historical_data;
use crate::standardized_types::time_slices::TimeSlice;
use crate::strategies::client_features::server_connections::set_warmup_complete;
use crate::strategies::handlers::indicator_handler::IndicatorHandler;
use crate::strategies::handlers::market_handler::price_service::{get_price_service_sender, PriceServiceMessage};
use crate::strategies::handlers::subscription_handler::SubscriptionHandler;
use crate::strategies::handlers::timed_events_handler::TimedEventHandler;
use crate::strategies::ledgers::ledger_service::LedgerService;
use crate::strategies::strategy_events::StrategyEvent;

pub(crate) async fn live_warm_up(
    warm_up_start_time: DateTime<Utc>,
    buffer_duration: Duration,
    tick_over_no_data: bool,
    subscription_handler: Arc<SubscriptionHandler>,
    strategy_event_sender: Sender<StrategyEvent>,
    timed_event_handler: Arc<TimedEventHandler>,
    ledger_service: Arc<LedgerService>,
    indicator_handler: Arc<IndicatorHandler>
) {
    println!("Historical Engine: Warming up the strategy...");
    let market_price_sender = get_price_service_sender();
    // here we are looping through 1 month at a time, if the strategy updates its subscriptions we will stop the data feed, download the historical data again to include updated symbols, and resume from the next time to be processed.
    let mut primary_subscriptions = subscription_handler.primary_subscriptions().await;
    let mut primary_subscription_update_receiver = subscription_handler.subscribe_primary_subscription_updates().await;
    for subscription in &primary_subscriptions {
        println!("Historical Engine: Primary Subscription: {}", subscription);
    }
    let strategy_subscriptions = subscription_handler.strategy_subscriptions().await;
    for subscription in &strategy_subscriptions {
        println!("Historical Engine: Strategy Subscription: {}", subscription);
    }
    let mut last_time = warm_up_start_time.clone();
    let mut first_iteration = true;
    'main_loop: loop {
        if !first_iteration {
            last_time += Duration::from_nanos(1)
        }
        let to_time: DateTime<Utc> = {
            let end_of_day_naive = last_time.date_naive().and_time(NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap());
            Utc.from_utc_datetime(&end_of_day_naive).max(last_time)
        };
        if first_iteration {
            first_iteration = false;
        }
        let mut time_slices = match get_historical_data(primary_subscriptions.clone(), last_time.clone(), to_time).await {
            Ok(time_slices) => {
                if time_slices.is_empty() && tick_over_no_data {
                    println!("Warm Up Engine: No data period, weekend or holiday: ticking through at buffering resolution, data will resume shortly");
                } else if time_slices.is_empty() && !tick_over_no_data {
                    last_time = to_time + buffer_duration;
                    continue 'main_loop
                }
                if to_time.day() == Utc::now().day() {
                    set_warmup_complete();
                    let event = StrategyEvent::WarmUpComplete;
                    match strategy_event_sender.send(event).await {
                        Ok(_) => {}
                        Err(e) => eprintln!("Historical Engine: Failed to send event: {}", e)
                    }
                    break 'main_loop
                }
                time_slices
            },
            Err(e) => {
                if tick_over_no_data {
                    println!("Historical Engine: Error getting data: {}", e);
                } else if !tick_over_no_data {
                    last_time = to_time + buffer_duration;
                    continue 'main_loop
                }
                BTreeMap::new()
            }
        };

        let mut time = last_time;
        'day_loop: while time <= to_time {
            time += buffer_duration;

            if time >= Utc::now() {
                set_warmup_complete();
                let event = StrategyEvent::WarmUpComplete;
                match strategy_event_sender.send(event).await {
                    Ok(_) => {}
                    Err(e) => eprintln!("Historical Engine: Failed to send event: {}", e)
                }
                break 'main_loop
            }

            // we interrupt if we have a new subscription event so we can fetch the correct data, we will resume from the last time processed.
            match primary_subscription_update_receiver.try_recv() {
                Ok(updates) => {
                    if updates != primary_subscriptions {
                        primary_subscriptions = updates;
                        break 'day_loop
                    }
                }
                Err(_) => {}
            }
            timed_event_handler.update_time(time.clone()).await;

            let time_range = last_time.timestamp_nanos_opt().unwrap()..=time.timestamp_nanos_opt().unwrap();
            let mut time_slice: TimeSlice = TimeSlice::new();

            // Extract and remove data points in this range
            let keys_to_remove: Vec<i64> = time_slices
                .range(time_range)
                .map(|(k, _)| *k)
                .collect();

            for key in keys_to_remove {
                if let Some(data) = time_slices.remove(&key) {
                    time_slice.extend(data);
                }
            }

            let mut strategy_time_slice: TimeSlice = TimeSlice::new();
            // update our consolidators and create the strategies time slice with any new data or just create empty slice.
            if !time_slice.is_empty() {
                let arc_slice = Arc::new(time_slice.clone());
                match market_price_sender.send(PriceServiceMessage::TimeSliceUpdate(arc_slice.clone())).await {
                    Ok(_) => {}
                    Err(e) => panic!("Market Handler: Error sending backtest message: {}", e)
                }
                ledger_service.timeslice_updates(time, arc_slice.clone()).await;

                // Add only primary data which the strategy has subscribed to into the strategies time slice
                if let Some(consolidated_data) = subscription_handler.update_time_slice(arc_slice.clone()).await {
                    strategy_time_slice.extend(consolidated_data);
                }

                strategy_time_slice.extend(time_slice);
            }

            // update the consolidators time and see if that generates new data, in case we didn't have primary data to update with.
            if let Some(consolidated_data) = subscription_handler.update_consolidators_time(time.clone()).await {
                strategy_time_slice.extend(consolidated_data);
            }

            if !strategy_time_slice.is_empty() {
                // Update indicators and get any generated events.
                indicator_handler.update_time_slice(&strategy_time_slice).await;

                let slice_event = StrategyEvent::TimeSlice(
                    strategy_time_slice,
                );
                match strategy_event_sender.send(slice_event).await {
                    Ok(_) => {}
                    Err(e) => eprintln!("Historical Engine: Failed to send event: {}", e)
                }
            }

            last_time = time.clone();
        }
    }
}