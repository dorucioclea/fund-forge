use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use rust_decimal::{Decimal, MathematicalOps};
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
pub struct AverageDirectionalIndex {
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
    adx_color: Color,
    plus_di_color: Color,
    minus_di_color: Color,
    period: u64,           // Typically 14 periods
    tick_rounding: bool,
    last_tr: Option<Decimal>,
    last_plus_dm: Option<Decimal>,
    last_minus_dm: Option<Decimal>,
    last_adx: Option<Decimal>,
    smoothed_tr: Option<Decimal>,
    smoothed_plus_dm: Option<Decimal>,
    smoothed_minus_dm: Option<Decimal>,
}

impl Display for AverageDirectionalIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let last = self.history.last();
        match last {
            Some(last) => write!(f, "{}\n{}", &self.name, last),
            None => write!(f, "{}: No Values", &self.name),
        }
    }
}

impl AverageDirectionalIndex {
    #[allow(dead_code)]
    pub async fn new(
        name: IndicatorName,
        subscription: DataSubscription,
        history_to_retain: usize,
        period: u64,
        adx_color: Color,
        plus_di_color: Color,
        minus_di_color: Color,
        tick_rounding: bool,
    ) -> Self {
        let symbol_name = match subscription.market_type {
            MarketType::Futures(_) => extract_symbol_from_contract(&subscription.symbol.name),
            _ => subscription.symbol.name.clone(),
        };
        let decimal_accuracy = subscription.symbol.data_vendor.decimal_accuracy(symbol_name.clone()).await.unwrap();
        let tick_size = subscription.symbol.data_vendor.tick_size(symbol_name.clone()).await.unwrap();

        let adx = AverageDirectionalIndex {
            name,
            market_type: subscription.symbol.market_type.clone(),
            subscription,
            history: RollingWindow::new(history_to_retain),
            base_data_history: RollingWindow::new(2), // Need current and previous bar
            is_ready: false,
            tick_size,
            adx_color,
            plus_di_color,
            minus_di_color,
            period,
            decimal_accuracy,
            tick_rounding,
            last_tr: None,
            last_plus_dm: None,
            last_minus_dm: None,
            last_adx: None,
            smoothed_tr: None,
            smoothed_plus_dm: None,
            smoothed_minus_dm: None,
        };
        adx
    }

    fn get_bar_data(data: &BaseDataEnum) -> (Price, Price, Price) {
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
            _ => panic!("Unsupported data type for ADX"),
        }
    }

    fn calculate_directional_movement(&self, curr_data: &BaseDataEnum, prev_data: &BaseDataEnum) -> (Price, Price, Price) {
        let (curr_high, curr_low, curr_close) = Self::get_bar_data(curr_data);
        let (prev_high, prev_low, prev_close) = Self::get_bar_data(prev_data);

        // Calculate True Range
        let tr = {
            let high_low = curr_high - curr_low;
            let high_close = (curr_high - prev_close).abs();
            let low_close = (curr_low - prev_close).abs();
            high_low.max(high_close).max(low_close)
        };

        // Calculate Plus and Minus Directional Movement
        let plus_dm = if curr_high - prev_high > prev_low - curr_low {
            (curr_high - prev_high).max(dec!(0.0))
        } else {
            dec!(0.0)
        };

        let minus_dm = if prev_low - curr_low > curr_high - prev_high {
            (prev_low - curr_low).max(dec!(0.0))
        } else {
            dec!(0.0)
        };

        (tr, plus_dm, minus_dm)
    }

    fn smooth_value(prev_smooth: Decimal, curr_value: Decimal, period: u64) -> Price {
        let period_dec = Decimal::from(period);
        ((period_dec - dec!(1.0)) * prev_smooth + curr_value) / period_dec
    }

    fn calculate_di(smoothed_dm: Decimal, smoothed_tr: Decimal) -> Price {
        if smoothed_tr == dec!(0.0) {
            dec!(0.0)
        } else {
            (smoothed_dm / smoothed_tr) * dec!(100.0)
        }
    }

    fn round_value(&self, value: Decimal) -> Price {
        match self.tick_rounding {
            true => round_to_tick_size(value, self.tick_size),
            false => value.round_dp(self.decimal_accuracy),
        }
    }
}

impl Indicators for AverageDirectionalIndex {
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
        if self.base_data_history.len() < 2 {
            return None;
        }

        let base_data = self.base_data_history.history();
        let (tr, plus_dm, minus_dm) = self.calculate_directional_movement(
            &base_data[1],
            &base_data[0]
        );

        // Initialize or update smoothed values
        let (smoothed_tr, smoothed_plus_dm, smoothed_minus_dm) = if self.smoothed_tr.is_none() {
            (tr, plus_dm, minus_dm)
        } else {
            (
                Self::smooth_value(self.smoothed_tr.unwrap(), tr, self.period),
                Self::smooth_value(self.smoothed_plus_dm.unwrap(), plus_dm, self.period),
                Self::smooth_value(self.smoothed_minus_dm.unwrap(), minus_dm, self.period),
            )
        };

        // Calculate +DI and -DI
        let plus_di = self.round_value(Self::calculate_di(smoothed_plus_dm, smoothed_tr));
        let minus_di = self.round_value(Self::calculate_di(smoothed_minus_dm, smoothed_tr));

        // Calculate ADX
        let di_diff = (plus_di - minus_di).abs();
        let di_sum = plus_di + minus_di;
        let dx = if di_sum == dec!(0.0) {
            dec!(0.0)
        } else {
            self.round_value((di_diff / di_sum) * dec!(100.0))
        };

        // Smooth ADX
        let adx = if self.last_adx.is_none() {
            dx
        } else {
            self.round_value(Self::smooth_value(self.last_adx.unwrap(), dx, self.period))
        };

        // Update state
        self.smoothed_tr = Some(smoothed_tr);
        self.smoothed_plus_dm = Some(smoothed_plus_dm);
        self.smoothed_minus_dm = Some(smoothed_minus_dm);
        self.last_adx = Some(adx);
        self.is_ready = true;

        // Create plots
        let mut plots = BTreeMap::new();
        plots.insert(
            "adx".to_string(),
            IndicatorPlot::new("ADX".to_string(), adx, self.adx_color.clone()),
        );
        plots.insert(
            "plus_di".to_string(),
            IndicatorPlot::new("+DI".to_string(), plus_di, self.plus_di_color.clone()),
        );
        plots.insert(
            "minus_di".to_string(),
            IndicatorPlot::new("-DI".to_string(), minus_di, self.minus_di_color.clone()),
        );

        let values = IndicatorValues::new(
            self.name.clone(),
            self.subscription.clone(),
            plots,
            base_data[1].time_closed_utc(),
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
        self.last_tr = None;
        self.last_plus_dm = None;
        self.last_minus_dm = None;
        self.last_adx = None;
        self.smoothed_tr = None;
        self.smoothed_plus_dm = None;
        self.smoothed_minus_dm = None;
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
        self.history.len() as u64 + self.period + 1
    }
}