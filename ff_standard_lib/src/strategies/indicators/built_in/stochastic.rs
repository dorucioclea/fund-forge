use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::gui_types::settings::Color;
use crate::helpers::decimal_calculators::round_to_tick_size;
use crate::product_maps::rithmic::maps::extract_symbol_from_contract;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::traits::BaseData;
use crate::standardized_types::enums::MarketType;
use crate::standardized_types::new_types::Price;
use crate::standardized_types::rolling_window::RollingWindow;
use crate::standardized_types::subscriptions::DataSubscription;
use crate::strategies::indicators::indicator_values::{IndicatorPlot, IndicatorValues};
use crate::strategies::indicators::indicators_trait::{IndicatorName, Indicators};

#[derive(Clone, Debug)]
pub struct StochasticOscillator {
    name: IndicatorName,
    subscription: DataSubscription,
    history: RollingWindow<IndicatorValues>,
    base_data_history: RollingWindow<BaseDataEnum>,
    #[allow(unused)]
    market_type: MarketType,
    #[allow(unused)]
    tick_size: Decimal,
    decimal_accuracy: u32,
    is_ready: bool,
    k_color: Color,
    d_color: Color,
    k_period: u64,     // %K period (typically 14)
    d_period: u64,     // %D period (typically 3)
    tick_rounding: bool,
    last_k_values: Vec<Decimal>, // Store recent %K values for %D calculation
}

impl Display for StochasticOscillator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let last = self.history.last();
        match last {
            Some(last) => write!(f, "{}\n{}", &self.name, last),
            None => write!(f, "{}: No Values", &self.name),
        }
    }
}

impl StochasticOscillator {
    #[allow(dead_code)]
    pub async fn new(
        name: IndicatorName,
        subscription: DataSubscription,
        history_to_retain: usize,
        k_period: u64,
        d_period: u64,
        k_color: Color,
        d_color: Color,
        tick_rounding: bool,
    ) -> Self {
        let symbol_name = match subscription.market_type {
            MarketType::Futures(_) => extract_symbol_from_contract(&subscription.symbol.name),
            _ => subscription.symbol.name.clone(),
        };
        let decimal_accuracy = subscription.symbol.data_vendor.decimal_accuracy(symbol_name.clone()).await.unwrap();
        let tick_size = subscription.symbol.data_vendor.tick_size(symbol_name.clone()).await.unwrap();

        let stoch = StochasticOscillator {
            name,
            market_type: subscription.symbol.market_type.clone(),
            subscription,
            history: RollingWindow::new(history_to_retain),
            base_data_history: RollingWindow::new(k_period as usize),
            is_ready: false,
            tick_size,
            k_color,
            d_color,
            k_period,
            d_period,
            decimal_accuracy,
            tick_rounding,
            last_k_values: Vec::with_capacity(d_period as usize),
        };
        stoch
    }

    fn get_price_data(data: &BaseDataEnum) -> (Price, Price, Price) {
        match data {
            BaseDataEnum::QuoteBar(bar) => (
                bar.bid_high,
                bar.bid_low,
                bar.bid_close,
            ),
            BaseDataEnum::Candle(candle) => (
                candle.high,
                candle.low,
                candle.close,
            ),
            _ => panic!("Unsupported data type for Stochastic Oscillator"),
        }
    }

    fn calculate_k(&self) -> Price {
        let base_data = self.base_data_history.history();

        // Find highest high and lowest low over the period
        let mut highest_high = dec!(0.0);
        let mut lowest_low = Decimal::MAX;

        for data in base_data.iter() {
            let (high, low, _) = Self::get_price_data(data);
            highest_high = highest_high.max(high);
            lowest_low = lowest_low.min(low);
        }

        // Get current close
        if let Some(last_data) = base_data.last() {
            let (_, _, close) = Self::get_price_data(last_data);

            // Calculate %K
            if highest_high == lowest_low {
                return dec!(50.0); // Middle value when range is zero
            }

            let k = (close - lowest_low) * dec!(100.0) / (highest_high - lowest_low);

            match self.tick_rounding {
                true => round_to_tick_size(k, self.tick_size),
                false => k.round_dp(self.decimal_accuracy),
            }
        } else {
            dec!(0.0)
        }
    }

    fn calculate_d(&self) -> Price {
        if self.last_k_values.len() < self.d_period as usize {
            return dec!(0.0);
        }

        let sum: Decimal = self.last_k_values.iter().sum();
        let d = sum / Decimal::from(self.d_period);

        match self.tick_rounding {
            true => round_to_tick_size(d, self.tick_size),
            false => d.round_dp(self.decimal_accuracy),
        }
    }
}

impl Indicators for StochasticOscillator {
    fn name(&self) -> IndicatorName {
        self.name.clone()
    }

    fn history_to_retain(&self) -> usize {
        self.history.number.clone() as usize
    }

    fn update_base_data(&mut self, base_data: &BaseDataEnum) -> Option<Vec<IndicatorValues>> {
        if !base_data.is_closed() {
            return None;
        }

        self.base_data_history.add(base_data.clone());

        if !self.is_ready {
            if !self.base_data_history.is_full() {
                return None;
            }
            self.is_ready = true;
        }

        // Calculate %K
        let k = self.calculate_k();
        if k == dec!(0.0) {
            return None;
        }

        // Update %K history and calculate %D
        self.last_k_values.push(k);
        if self.last_k_values.len() > self.d_period as usize {
            self.last_k_values.remove(0);
        }
        let d = self.calculate_d();

        // Create plots
        let mut plots = BTreeMap::new();
        plots.insert(
            "k".to_string(),
            IndicatorPlot::new("%K".to_string(), k, self.k_color.clone()),
        );

        if d > dec!(0.0) {
            plots.insert(
                "d".to_string(),
                IndicatorPlot::new("%D".to_string(), d, self.d_color.clone()),
            );
        }

        let values = IndicatorValues::new(
            self.name.clone(),
            self.subscription.clone(),
            plots,
            base_data.time_closed_utc(),
        );

        self.history.add(values.clone());
        Some(vec![values])
    }

    fn subscription(&self) -> &DataSubscription {
        &self.subscription
    }

    fn reset(&mut self) {
        self.history.clear();
        self.base_data_history.clear();
        self.is_ready = false;
        self.last_k_values.clear();
    }

    fn index(&self, index: usize) -> Option<IndicatorValues> {
        if !self.is_ready {
            return None;
        }
        self.history.get(index).cloned()
    }

    fn current(&self) -> Option<IndicatorValues> {
        if !self.is_ready {
            return None;
        }
        self.history.last().cloned()
    }

    fn plots(&self) -> RollingWindow<IndicatorValues> {
        self.history.clone()
    }

    fn is_ready(&self) -> bool {
        self.is_ready
    }

    fn history(&self) -> RollingWindow<IndicatorValues> {
        self.history.clone()
    }

    fn data_required_warmup(&self) -> u64 {
        self.history.len() as u64 + self.k_period + self.d_period
    }
}