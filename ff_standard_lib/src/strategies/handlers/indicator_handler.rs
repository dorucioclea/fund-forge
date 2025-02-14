use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::Arc;
use crate::standardized_types::enums::StrategyMode;
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::subscriptions::DataSubscription;
use crate::standardized_types::time_slices::TimeSlice;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use crate::strategies::consolidators::consolidator_enum::ConsolidatorEnum;
use crate::strategies::indicators::indicator_events::IndicatorEvents;
use crate::strategies::indicators::indicators_trait::{IndicatorName, Indicators};
use crate::strategies::indicators::indicator_values::IndicatorValues;
use crate::strategies::client_features::server_connections::is_warmup_complete;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::base_data::traits::BaseData;
use crate::standardized_types::market_hours::TradingHours;
use crate::strategies::handlers::subscription_handler::SubscriptionHandler;

pub struct IndicatorHandler {
    indicators: Arc<DashMap<DataSubscription, DashMap<IndicatorName, Box<dyn Indicators>>>>,
    strategy_mode: StrategyMode,
    subscription_map: DashMap<IndicatorName, DataSubscription>, //used to quickly find the subscription of an indicator by name.
    subscription_handler: Arc<SubscriptionHandler>,
}

impl IndicatorHandler {
    pub async fn new(strategy_mode: StrategyMode, subscription_handler: Arc<SubscriptionHandler>) -> Self {
        let handler =Self {
            indicators: Default::default(),
            strategy_mode,
            subscription_map: Default::default(),
            subscription_handler,
        };
        handler
    }

    pub async fn add_indicator(&self, indicator: Box<dyn Indicators>, time: DateTime<Utc>, market_hours: Option<TradingHours>) -> IndicatorEvents {
        let subscription = indicator.subscription().clone();

        if !self.indicators.contains_key(&subscription) {
            self.indicators.insert(subscription.clone(), DashMap::new());
        }

        let name = indicator.name().clone();

        let indicator = match is_warmup_complete() {
            true => warmup(time, self.strategy_mode.clone(), indicator, self.subscription_handler.clone(), market_hours).await,
            false => indicator,
        };

        let event = if !self.subscription_map.contains_key(&name) {
            IndicatorEvents::IndicatorAdded(name.clone())
        } else {
           IndicatorEvents::Replaced(name.clone())
        };

        if let Some(map) = self.indicators.get(&subscription) {
            map.insert(indicator.name(), indicator);
        }
        self.subscription_map.insert(name.clone(), subscription.clone());

        event
    }

    pub async fn remove_indicator(&self, indicator_name: &IndicatorName) -> Option<IndicatorEvents>  {
        if let Some(subscription) = self.subscription_map.get(indicator_name) {
            if let Some(map) = self.indicators.get(&subscription.value()) {
                map.remove(indicator_name);
            }
        }
        match self.subscription_map.remove(indicator_name) {
            None => None,
            Some(_) => Some(IndicatorEvents::IndicatorRemoved(indicator_name.clone()))
        }
    }

    pub async fn indicators_unsubscribe_subscription(&self, subscription: &DataSubscription) {
        self.indicators.remove(subscription);
        for sub in self.subscription_map.iter() {
            if sub.value() == subscription {
                self.subscription_map.remove(sub.key());
            }
        }
    }

    pub async fn update_time_slice(&self, time_slice: &TimeSlice) -> Option<IndicatorEvents> {
        let mut results: BTreeMap<IndicatorName, Vec<IndicatorValues>> = BTreeMap::new();
        let indicators = self.indicators.clone();

        for data in time_slice.iter() {
            let subscription = data.subscription();
            if let Some(indicators_by_sub) = indicators.get_mut(&subscription) {
                for mut indicators_dash_map in indicators_by_sub.iter_mut() {
                    if let Some(indicator_data) = indicators_dash_map.value_mut().update_base_data(data) {
                        results.entry(indicators_dash_map.key().clone())
                            .or_insert_with(Vec::new)
                            .extend(indicator_data);
                    }
                }
            }
        }

        if !results.is_empty() {
            let results_vec: Vec<IndicatorValues> = results.into_values().flatten().collect();
            return Some(IndicatorEvents::IndicatorTimeSlice(results_vec))
        }
        None
    }

    pub async fn history(&self, name: IndicatorName) -> Option<RollingWindow<IndicatorValues>> {
        let subscription = match self.subscription_map.get(&name) {
            Some(sub) => sub.clone(),
            None => return None,
        };
        if let Some(map) = self.indicators.get(&subscription) {
            if let Some(indicator) = map.get(&name) {
                let history = indicator.history();
                return match history.is_empty() {
                    true => None,
                    false => Some(history),
                };
            }
        }
        None
    }

    pub fn current(&self, name: &IndicatorName) -> Option<IndicatorValues> {
        let subscription = match self.subscription_map.get(name) {
            Some(sub) => sub.clone(),
            None => return None,
        };
        if let Some(map) = self.indicators.get(&subscription) {
            for indicator in map.value() {
                if &indicator.name() == name {
                    return indicator.current();
                }
            }
        }
        None
    }

    pub fn index(&self, name: &IndicatorName, index: usize) -> Option<IndicatorValues> {
        let subscription = match self.subscription_map.get(name) {
            Some(sub) => sub.clone(),
            None => return None,
        };
        if let Some(map) = self.indicators.get(&subscription) {
            for indicator in map.value() {
                if &indicator.name() == name {
                    return indicator.index(index);
                }
            }
        }
        None
    }
}

/// This will warm up the indicator if possible.
/// Indicators that use fundamental data will need to be managed manually.
async fn warmup( //todo make async task version for live mode
    to_time: DateTime<Utc>,
    strategy_mode: StrategyMode,
    mut indicator: Box<dyn Indicators>,
     subscription_handler: Arc<SubscriptionHandler>,
     market_hours: Option<TradingHours>,
) -> Box<dyn Indicators> {
   //1. Check if we have history for the indicator.subscription
    let subscription =  indicator.subscription();
    match subscription.base_data_type {
        BaseDataType::Ticks => {
            if let Some(history) = subscription_handler.tick_history(&subscription) {
                if history.len() >= indicator.data_required_warmup() as usize {
                    for data in history.history {
                        let base_data = BaseDataEnum::Tick(data);
                        indicator.update_base_data(&base_data);
                    }
                    return indicator
                }
            }
        }
        BaseDataType::Quotes => {
            if let Some(history) = subscription_handler.quote_history(&subscription) {
                if history.len() >= indicator.data_required_warmup() as usize {
                    for data in history.history {
                        let base_data = BaseDataEnum::Quote(data);
                        indicator.update_base_data(&base_data);
                    }
                    return indicator
                }
            }
        }
        BaseDataType::QuoteBars => {
            if let Some(history) = subscription_handler.bar_history(&subscription) {
                if history.len() >= indicator.data_required_warmup() as usize {
                    for data in history.history {
                        let base_data = BaseDataEnum::QuoteBar(data);
                        indicator.update_base_data(&base_data);
                    }
                    return indicator
                }

            }
        }
        BaseDataType::Candles => {
            if let Some(history) = subscription_handler.candle_history(&subscription) {
                if history.len() >= indicator.data_required_warmup() as usize {
                    for data in history.history {
                        let base_data = BaseDataEnum::Candle(data);
                        indicator.update_base_data(&base_data);
                    }
                    return indicator
                }
            }
        }
        _ => {}
    }
    let _ = subscription_handler.deref();
    let consolidator = ConsolidatorEnum::create_consolidator(subscription.clone(), false, market_hours).await;
    let (_, window) = ConsolidatorEnum::warmup(consolidator, to_time, (indicator.data_required_warmup() + 1) as i32, strategy_mode).await;
    for data in window.history {
        let _ = indicator.update_base_data(&data);
    }
    indicator
}
