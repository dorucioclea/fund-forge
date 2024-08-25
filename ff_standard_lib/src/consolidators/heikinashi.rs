use chrono::{DateTime, Duration, Utc};
use crate::apis::vendor::client_requests::ClientSideDataVendor;
use crate::consolidators::candlesticks::open_time;
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::enums::{Resolution, StrategyMode};
use crate::standardized_types::subscriptions::{CandleType, DataSubscription};
use crate::consolidators::count::ConsolidatorError;
use crate::helpers::decimal_calculators::round_to_tick_size;
use crate::standardized_types::base_data::candle::Candle;
use crate::standardized_types::base_data::history::range_data;
use crate::standardized_types::base_data::traits::BaseData;

pub struct HeikinAshiConsolidator{
    current_data: Option<BaseDataEnum>,
    pub(crate) subscription: DataSubscription,
    pub(crate) history: RollingWindow<BaseDataEnum>,
    previous_ha_close: f64,
    previous_ha_open: f64,
    tick_size: f64,
}

impl HeikinAshiConsolidator
{
    fn candle_from_base_data(&self, ha_open: f64, ha_high: f64, ha_low: f64, ha_close: f64, volume: f64, time: String, is_closed: bool, range: f64) -> Candle {
        Candle {
            symbol: self.subscription.symbol.clone(),
            open: ha_open,
            high: ha_high,
            low: ha_low,
            close: ha_close,
            volume,
            time,
            resolution: self.subscription.resolution.clone(),
            is_closed,
            range,
            candle_type: CandleType::HeikinAshi,
        }
    }

    fn new_heikin_ashi_candle(&mut self, new_data: &BaseDataEnum) -> Candle {
        match new_data {
            BaseDataEnum::Candle(candle) => {
                if self.previous_ha_close == 0.0 && self.previous_ha_open == 0.0 {
                    self.previous_ha_close = candle.close;
                    self.previous_ha_open = candle.open;
                }
                let ha_close = round_to_tick_size((candle.open + candle.high + candle.low + candle.close) / 4.0, self.tick_size);
                let ha_open = round_to_tick_size((self.previous_ha_open + self.previous_ha_close) / 2.0, self.tick_size);
                let ha_high = candle.high.max(ha_open).max(ha_close);
                let ha_low = candle.low.min(ha_open).min(ha_close);

                // Update previous Heikin Ashi values for next bar
                self.previous_ha_close = ha_close;
                self.previous_ha_open = ha_open;
                let time = open_time(&self.subscription, new_data.time_utc());

                self.candle_from_base_data(ha_open, ha_high, ha_low, ha_close, candle.volume, time.to_string(), false, ha_high - ha_low)
            },
            BaseDataEnum::Price(price) => {
                if self.previous_ha_close == 0.0 && self.previous_ha_open == 0.0 {
                    self.previous_ha_close = price.price;
                    self.previous_ha_open = price.price;
                }
                let ha_close = price.price;
                let ha_open = round_to_tick_size((self.previous_ha_open + self.previous_ha_close) / 2.0, self.tick_size);
                let ha_high = ha_close.max(ha_open);
                let ha_low = ha_close.min(ha_open);

                // Update previous Heikin Ashi values for next bar
                self.previous_ha_close = ha_close;
                self.previous_ha_open = ha_open;
                let time = open_time(&self.subscription, new_data.time_utc());

                self.candle_from_base_data(ha_open, ha_high, ha_low, ha_close, 0.0, time.to_string(), false, ha_high - ha_low)
            },
            BaseDataEnum::QuoteBar(bar) => {
                if self.previous_ha_close == 0.0 && self.previous_ha_open == 0.0 {
                    self.previous_ha_close = bar.bid_close;
                    self.previous_ha_open = bar.bid_close;
                }
                let ha_close = bar.bid_close;
                let ha_open = round_to_tick_size((self.previous_ha_open + self.previous_ha_close) / 2.0, self.tick_size);
                let ha_high = ha_close.max(ha_open);
                let ha_low = ha_close.min(ha_open);

                // Update previous Heikin Ashi values for next bar
                self.previous_ha_close = ha_close;
                self.previous_ha_open = ha_open;
                let time = open_time(&self.subscription, new_data.time_utc());

                self.candle_from_base_data(ha_open, ha_high, ha_low, ha_close, bar.volume, time.to_string(), false, ha_high - ha_low)
            },
            BaseDataEnum::Tick(tick) => {
                if self.previous_ha_close == 0.0 && self.previous_ha_open == 0.0 {
                    self.previous_ha_close = tick.price;
                    self.previous_ha_open = tick.price;
                }
                let ha_close = tick.price;
                let ha_open = round_to_tick_size((self.previous_ha_open + self.previous_ha_close) / 2.0, self.tick_size);
                let ha_high = ha_close.max(ha_open);
                let ha_low = ha_close.min(ha_open);

                // Update previous Heikin Ashi values for next bar
                self.previous_ha_close = ha_close;
                self.previous_ha_open = ha_open;
                let time = open_time(&self.subscription, new_data.time_utc());

                self.candle_from_base_data(ha_open, ha_high, ha_low, ha_close, tick.volume, time.to_string(), false, ha_high - ha_low)
            },
            _ => panic!("Invalid base data type for Heikin Ashi calculation")
        }
    }
}

impl HeikinAshiConsolidator
{
    pub(crate) async fn new(subscription: DataSubscription, history_to_retain: u64) -> Result<HeikinAshiConsolidator, ConsolidatorError> {
        if subscription.base_data_type != BaseDataType::Candles {
            return Err(ConsolidatorError { message: format!("{} is an Invalid base data type for HeikinAshiConsolidator", subscription.base_data_type) });
        }

        if let Some(candle_type) = &subscription.candle_type {
            if candle_type != &CandleType::HeikinAshi {
                return Err(ConsolidatorError { message: format!("{:?} is an Invalid candle type for HeikinAshiConsolidator", candle_type) });
            }
        }
        let tick_size = match subscription.symbol.data_vendor.tick_size(subscription.symbol.clone()).await {
            Ok(size) => size,
            Err(e) => return Err(ConsolidatorError { message: format!("Error getting tick size: {}", e) }),
        };
        
        Ok(HeikinAshiConsolidator {
            current_data: None,
            subscription,
            history: RollingWindow::new(history_to_retain),
            previous_ha_close: 0.0,
            previous_ha_open: 0.0,
            tick_size
        })
    }

    pub(crate) async fn new_and_warmup(subscription: DataSubscription, history_to_retain: u64, warm_up_to_time: DateTime<Utc>, strategy_mode: StrategyMode) -> Result<HeikinAshiConsolidator, ConsolidatorError> {
        if subscription.base_data_type != BaseDataType::Candles {
            return Err(ConsolidatorError { message: format!("{} is an Invalid base data type for HeikinAshiConsolidator", subscription.base_data_type) });
        }
        if let Resolution::Ticks(_) = subscription.resolution {
            return Err(ConsolidatorError { message: format!("{:?} is an Invalid resolution for TimeConsolidator", subscription.resolution) });
        }

        let tick_size = match subscription.symbol.data_vendor.tick_size(subscription.symbol.clone()).await {
            Ok(size) => size,
            Err(e) => return Err(ConsolidatorError { message: format!("Error getting tick size: {}", e) }),
        };
        let mut consolidator = HeikinAshiConsolidator{
            current_data: None,
            subscription,
            history: RollingWindow::new(history_to_retain),
            previous_ha_close: 0.0,
            previous_ha_open: 0.0,
            tick_size
        };
        consolidator.warmup(warm_up_to_time, strategy_mode).await;
        Ok(consolidator)
    }

    pub fn update_time(&mut self, time: DateTime<Utc>) -> Vec<BaseDataEnum> {
        if let Some(current_data) = self.current_data.as_mut() {
            if time >= current_data.time_created_utc() {
                let return_data = current_data.clone();
                self.current_data = None;
                return vec![return_data];
            }
        }
        vec![]
    }
    
    //problem where this is returning a closed candle constantly
    pub(crate) fn update(&mut self, base_data: &BaseDataEnum) -> Vec<BaseDataEnum> {
        if self.current_data.is_none() {
            let data = self.new_heikin_ashi_candle(base_data);
            self.current_data = Some(BaseDataEnum::Candle(data));
            let candles = vec![self.current_data.clone().unwrap()];
            return candles;
        } else if let Some(current_bar) = self.current_data.as_mut() {
            if base_data.time_created_utc() >= current_bar.time_created_utc() {
                let mut consolidated_bar = current_bar.clone();
                consolidated_bar.set_is_closed(true);
                self.history.add(consolidated_bar.clone());

                let new_bar = self.new_heikin_ashi_candle(base_data);
                self.current_data = Some(BaseDataEnum::Candle(new_bar.clone()));
                return vec![consolidated_bar, BaseDataEnum::Candle(new_bar)];
            }
            match current_bar {
                BaseDataEnum::Candle(candle) => {
                    match base_data {
                        BaseDataEnum::Tick(tick) => {
                            candle.high = tick.price.max(candle.high);
                            candle.low = tick.price.min(candle.low);
                            candle.close = tick.price;
                            candle.range = candle.high - candle.low;
                            candle.volume += tick.volume;
                            return vec![BaseDataEnum::Candle(candle.clone())];
                        }
                        BaseDataEnum::Candle(new_candle) => {
                            candle.high = new_candle.high.max(candle.high);
                            candle.low = new_candle.low.min(candle.low);
                            candle.close = new_candle.close;
                            candle.range = candle.high - candle.low;
                            candle.volume += new_candle.volume;
                            return vec![BaseDataEnum::Candle(candle.clone())];
                        }
                        BaseDataEnum::Price(price) => {
                            candle.high = price.price.max(candle.high);
                            candle.low = price.price.min(candle.low);
                            candle.close = price.price;
                            candle.range = candle.high - candle.low;
                            return vec![BaseDataEnum::Candle(candle.clone())];
                        }
                        BaseDataEnum::QuoteBar(bar) => {
                            candle.high = bar.bid_high.max(candle.high);
                            candle.low = bar.bid_low.min(candle.low);
                            candle.close = bar.bid_close;
                            candle.range = candle.high - candle.low;
                            candle.volume += bar.volume;
                            return vec![BaseDataEnum::Candle(candle.clone())];
                        }
                        _ => panic!("Invalid base data type for Heikin Ashi consolidator: {}", base_data.base_data_type()),
                    }
                }
                _ => panic!("Invalid base data type for Candle consolidator: {}", base_data.base_data_type()),
            }
        }
        panic!("Invalid base data type for Candle consolidator: {}", base_data.base_data_type())
    }

    pub(crate) fn clear_current_data(&mut self) {
        self.current_data = None;
        self.history.clear();
        self.previous_ha_close = 0.0;
        self.previous_ha_open = 0.0;
    }

    pub(crate) fn history(&self) -> RollingWindow<BaseDataEnum> {
        self.history.clone()
    }
    

    pub(crate) fn index(&self, index: u64) -> Option<BaseDataEnum> {
        match self.history.get(index) {
            Some(data) => Some(data.clone()),
            None => None,
        }
    }

    pub(crate) fn current(&self) -> Option<BaseDataEnum> {
        match &self.current_data {
            Some(data) => Some(data.clone()),
            None => None,
        }
    }

    async fn warmup(&mut self, to_time: DateTime<Utc>, strategy_mode: StrategyMode) {
        //todo if live we will tell the self.subscription.symbol.data_vendor to .update_historical_symbol()... we will wait then continue
        let vendor_resolutions = self.subscription.symbol.data_vendor.resolutions(self.subscription.market_type.clone()).await.unwrap();
        let mut minimum_resolution: Option<Resolution> = None;
        for resolution in vendor_resolutions {
            if minimum_resolution.is_none() {
                minimum_resolution = Some(resolution);
            } else {
                if resolution > minimum_resolution.unwrap() && resolution < self.subscription.resolution {
                    minimum_resolution = Some(resolution);
                }
            }
        }

        let minimum_resolution = match minimum_resolution.is_none() {
            true => panic!("{} does not have any resolutions available", self.subscription.symbol.data_vendor),
            false => minimum_resolution.unwrap()
        };

        let data_type = match minimum_resolution {
            Resolution::Ticks(_) => BaseDataType::Ticks,
            _ => self.subscription.base_data_type.clone()
        };

        let from_time = to_time - (self.subscription.resolution.as_duration() * self.history.number as i32) - Duration::days(4); //we go back a bit further in case of holidays or weekends

        let base_subscription = DataSubscription::new(self.subscription.symbol.name.clone(), self.subscription.symbol.data_vendor.clone(), minimum_resolution, data_type, self.subscription.market_type.clone());
        let base_data = range_data(from_time, to_time, base_subscription.clone()).await;

        for (_, slice) in &base_data {
            for base_data in slice {
                self.update(base_data);
            }
        }
        if strategy_mode != StrategyMode::Backtest {
            //todo() we will get any bars which are not in out serialized history here
        }
    }
}