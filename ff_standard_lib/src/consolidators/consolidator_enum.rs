use chrono::{DateTime, Utc};
use crate::consolidators::candlesticks::CandleStickConsolidator;
use crate::consolidators::count::{CountConsolidator};
use crate::consolidators::heikinashi::HeikinAshiConsolidator;
use crate::consolidators::renko::RenkoConsolidator;
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::enums::{Resolution, StrategyMode};
use crate::standardized_types::subscriptions::{CandleType, DataSubscription};


pub enum ConsolidatorEnum {
    Count(CountConsolidator),
    CandleStickConsolidator(CandleStickConsolidator),
    HeikinAshi(HeikinAshiConsolidator),
    Renko(RenkoConsolidator),
}

impl ConsolidatorEnum {
    
    /// Creates a new consolidator based on the subscription. if is_warmed_up is true, the consolidator will warm up to the to_time on its own.
    pub async fn create_consolidator(is_warmed_up: bool, subscription: DataSubscription, history_to_retain: u64, to_time: DateTime<Utc>, strategy_mode: StrategyMode) -> ConsolidatorEnum {
        //todo return a consolidator error instead of unwrap() so subscription manager can return DataSubscriptionEvent::Failed to the strategy and handle gracefully.
        let is_tick = match subscription.resolution {
            Resolution::Ticks(_) => true,
            _ => false
        };
        
        if  is_tick {
            return match is_warmed_up {
                true => ConsolidatorEnum::Count(CountConsolidator::new_and_warmup(subscription.clone(), history_to_retain, to_time, strategy_mode).await.unwrap()),
                false => ConsolidatorEnum::Count(CountConsolidator::new(subscription.clone(), history_to_retain).await.unwrap()),
            }
        }
        
        if is_warmed_up {
            return match &subscription.candle_type {
                Some(candle_type) => {
                    match candle_type {
                        CandleType::Renko(_) => ConsolidatorEnum::Renko(RenkoConsolidator::new_and_warmup(subscription.clone(), history_to_retain, to_time, strategy_mode).await.unwrap()),
                        CandleType::HeikinAshi => ConsolidatorEnum::HeikinAshi(HeikinAshiConsolidator::new_and_warmup(subscription.clone(), history_to_retain, to_time, strategy_mode).await.unwrap()),
                        CandleType::CandleStick => ConsolidatorEnum::CandleStickConsolidator(CandleStickConsolidator::new_and_warmup(subscription.clone(), history_to_retain, to_time, strategy_mode).await.unwrap())
                    }
                },
                None => ConsolidatorEnum::CandleStickConsolidator(CandleStickConsolidator::new_and_warmup(subscription.clone(), history_to_retain, to_time, strategy_mode).await.unwrap())
            }
        }
        
        match &subscription.candle_type {
            Some(candle_type) => {
                match candle_type {
                    CandleType::Renko(_) => ConsolidatorEnum::Renko(RenkoConsolidator::new(subscription.clone(), history_to_retain).await.unwrap()),
                    CandleType::HeikinAshi => ConsolidatorEnum::HeikinAshi(HeikinAshiConsolidator::new(subscription.clone(), history_to_retain).await.unwrap()),
                    CandleType::CandleStick => ConsolidatorEnum::CandleStickConsolidator(CandleStickConsolidator::new(subscription.clone(), history_to_retain).await.unwrap())
                }
            },
            None => panic!("Candle type is required for this subscription")
        }
    }

    /// Updates the consolidator with the new data point.
    pub async fn update(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => {
                count_consolidator.update(base_data).await
            },
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => {
                time_consolidator.update(base_data).await
            },
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => {
                heikin_ashi_consolidator.update(base_data).await
            }
            ConsolidatorEnum::Renko(renko_consolidator) => {
                renko_consolidator.update(base_data).await
            }
        }
    }

    /// Clears the current data and history of the consolidator.
    pub fn subscription(&self) -> &DataSubscription {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => &count_consolidator.subscription,
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => &time_consolidator.subscription,
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => &heikin_ashi_consolidator.subscription,
            ConsolidatorEnum::Renko(renko_consolidator) => &renko_consolidator.subscription,
        }
    }
    
    /// Returns the resolution of the consolidator.
    pub fn resolution(&self) -> &Resolution {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => &count_consolidator.subscription.resolution,
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => &time_consolidator.subscription.resolution,
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => &heikin_ashi_consolidator.subscription.resolution,
            ConsolidatorEnum::Renko(renko_consolidator) => &renko_consolidator.subscription.resolution,
        }
    }
    
    /// Returns the history to retain for the consolidator.
    pub fn history_to_retain(&self) -> u64 {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => count_consolidator.history.number,
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => time_consolidator.history.number,
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => heikin_ashi_consolidator.history.number,
            ConsolidatorEnum::Renko(renko_consolidator) => renko_consolidator.history.number,
        }
    }
    
    pub fn history(&self) -> RollingWindow<BaseDataEnum> {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => count_consolidator.history(),
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => time_consolidator.history(),
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => heikin_ashi_consolidator.history(),
            ConsolidatorEnum::Renko(renko_consolidator) => renko_consolidator.history(),
        }
    }
    
    pub fn current(&self) -> Option<BaseDataEnum> {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => count_consolidator.current(),
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => time_consolidator.current(),
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => heikin_ashi_consolidator.current(),
            ConsolidatorEnum::Renko(renko_consolidator) => renko_consolidator.current(),
        }
    }

    pub fn index(&self, index: u64) -> Option<BaseDataEnum> {
        match self {
            ConsolidatorEnum::Count(count_consolidator) => count_consolidator.index(index),
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => time_consolidator.index(index),
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => heikin_ashi_consolidator.index(index),
            ConsolidatorEnum::Renko(renko_consolidator) => renko_consolidator.index(index),
        }
    }
    
    pub async fn update_time(&mut self, time: DateTime<Utc>) -> Vec<BaseDataEnum> {
        match self {
            ConsolidatorEnum::Count(_) => vec![],
            ConsolidatorEnum::CandleStickConsolidator(time_consolidator) => time_consolidator.update_time(time).await,
            ConsolidatorEnum::HeikinAshi(heikin_ashi_consolidator) => heikin_ashi_consolidator.update_time(time).await,
            ConsolidatorEnum::Renko(_) => vec![],
        }
    }
}