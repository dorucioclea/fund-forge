use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
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

/// Moving Average (MA)
/// A trend-following indicator that smooths price data to create a single flowing line.
///
/// # Plots
/// - "ma": The main moving average line. Shows average price over the specified period.
///
/// # Parameters
/// - period: Number of periods to average (e.g., 20 for 20-period MA)
/// - tick_rounding: Whether to round values to tick size
///
/// # Usage
/// Helps identify trend direction and potential support/resistance levels.

/// Exponential Moving Average (EMA)
/// Similar to MA but gives more weight to recent prices.
///
/// # Plots
/// - "ema": The main EMA line. Responds more quickly to price changes than simple MA.
///
/// # Parameters
/// - period: Number of periods for the EMA calculation
/// - tick_rounding: Whether to round values to tick size
///
/// # Usage
/// More responsive to recent price changes than simple MA, better for shorter-term trading.
#[derive(Clone, Debug)]
pub struct MovingAverage {
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
    plot_color: Color,
    period: u64,
    tick_rounding: bool,
}

impl Display for MovingAverage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let last = self.history.last();
        match last {
            Some(last) => write!(f, "{}\n{}", &self.name, last),
            None => write!(f, "{}: No Values", &self.name),
        }
    }
}

impl MovingAverage {
    #[allow(dead_code)]
    pub async fn new(
        name: IndicatorName,
        subscription: DataSubscription,
        history_to_retain: usize,
        period: u64,
        plot_color: Color,
        tick_rounding: bool,
    ) -> Box<Self> {
        let symbol_name = match subscription.market_type {
            MarketType::Futures(_) => extract_symbol_from_contract(&subscription.symbol.name),
            _ => subscription.symbol.name.clone(),
        };
        let decimal_accuracy = subscription.symbol.data_vendor.decimal_accuracy(symbol_name.clone()).await.unwrap();
        let tick_size = subscription.symbol.data_vendor.tick_size(symbol_name.clone()).await.unwrap();

        let ma = MovingAverage {
            name,
            market_type: subscription.symbol.market_type.clone(),
            subscription,
            history: RollingWindow::new(history_to_retain),
            base_data_history: RollingWindow::new(period as usize),
            is_ready: false,
            tick_size,
            plot_color,
            period,
            decimal_accuracy,
            tick_rounding,
        };
        Box::new(ma)
    }

    fn calculate_average(&self) -> Price {
        let base_data = self.base_data_history.history();
        let values: Vec<Decimal> = base_data.iter()
            .map(|data| match data {
                BaseDataEnum::QuoteBar(bar) => bar.bid_close,
                BaseDataEnum::Candle(candle) => candle.close,
                _ => panic!("Unsupported data type for MovingAverage"),
            })
            .collect();

        if values.is_empty() {
            return dec!(0.0);
        }

        let sum: Decimal = values.iter().sum();
        if sum == dec!(0.0) {
            return dec!(0.0);
        }

        let average = match self.tick_rounding {
            true => round_to_tick_size(
                sum / Decimal::from_u64(values.len() as u64).unwrap(),
                self.tick_size
            ),
            false => (sum / Decimal::from_u64(values.len() as u64).unwrap())
                .round_dp(self.decimal_accuracy),
        };

        average
    }
}

impl Indicators for MovingAverage {
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
            } else {
                self.is_ready = true;
            }
        }

        let ma = self.calculate_average();
        if ma == dec!(0.0) {
            return None;
        }

        let mut plots = BTreeMap::new();
        let name = "ma".to_string();
        plots.insert(
            name,
            IndicatorPlot::new("ma".to_string(), ma, self.plot_color.clone()),
        );

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
        self.history.len() as u64 + self.period
    }
}