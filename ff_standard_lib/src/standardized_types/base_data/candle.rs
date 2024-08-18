use std::fmt;
use std::str::FromStr;
use chrono::{DateTime, FixedOffset};
use chrono_tz::Tz;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use crate::helpers::converters::time_local_from_str;
use crate::standardized_types::base_data::quotebar::QuoteBar;
use crate::standardized_types::enums::Resolution;
use crate::standardized_types::subscriptions::Symbol;

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Debug, Eq, PartialOrd, Ord)]
#[archive(
compare(PartialEq),
check_bytes,
)]
#[archive_attr(derive(Debug))]
pub enum CandleCalculationType {
    HeikenAshi,
    Candle
}

impl Default for CandleCalculationType {
    fn default() -> Self {
        CandleCalculationType::Candle
    }
}

/// Represents a single candlestick in a candlestick chart, commonly used
/// in the financial technical analysis of price patterns.
///
/// # Fields
///
/// - `symbol`: The trading symbol of the asset.
/// - `high`: The highest price.
/// - `low`: The lowest price.
/// - `open`: The opening price.
/// - `close`: The closing price.
/// - `volume`: The trading volume.
/// - `range`: The difference between the high and low prices.
/// - `time`: The opening time of the candles as a Unix timestamp.
/// - `is_closed`: Indicates whether the candles is closed.
/// - `data_vendor`: The data vendor that provided the candles.
/// - `resolution`: The resolution of the candles.
#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq)]
#[archive(
// This will generate a PartialEq impl between our unarchived and archived
// types:
compare(PartialEq),
// bytecheck can be used to validate your data if you want. To use the safe
// API, you have to derive CheckBytes for the archived type:
check_bytes,
)]
#[archive_attr(derive(Debug))]
pub struct Candle {
    pub symbol: Symbol,
    pub high: f64,
    pub low: f64,
    pub open: f64,
    pub close: f64,
    pub volume: f64,
    pub range: f64,
    pub time: String,
    pub is_closed: bool,
    pub resolution: Resolution,
}

impl Candle {

    pub fn from_quotebar(quotebar: QuoteBar,  bid: bool) -> Self {
        let high = match bid {
            true => quotebar.bid_high,
            false => quotebar.ask_high,
        };
        let low = match bid {
            true => quotebar.bid_low,
            false => quotebar.ask_low,
        };
        let open = match bid {
            true => quotebar.bid_open,
            false => quotebar.ask_open,
        };
        let close = match bid {
            true => quotebar.bid_close,
            false => quotebar.ask_close,
        };

        Candle {
            symbol: quotebar.symbol,
            time: quotebar.time,
            is_closed: quotebar.is_closed,
            open,
            high,
            low,
            close,
            volume: quotebar.volume,
            range: high - low,
            resolution: quotebar.resolution,
        }
    }
    /// Mutates the raw candle data into the required CandleStickType
    pub fn mutate_candle(&mut self, candle_calculation_type: &CandleCalculationType, prior_candle: &Option<Candle>) {
        match candle_calculation_type {
            CandleCalculationType::Candle => {
                return
            }
            CandleCalculationType::HeikenAshi => {
                if let Some(prior) = prior_candle {
                    let open = (prior.open + prior.close) / 2.0;
                    let close = (self.open + self.high + self.low + self.close) / 4.0;
                    let high = self.high.max(open).max(close);
                    let low = self.low.min(open).min(close);
                    self.open = open;
                    self.close = close;
                    self.high = high;
                    self.low = low;
                } else {
                    let open = (self.open + self.close) / 2.0;
                    let close = (self.open + self.high + self.low + self.close) / 4.0;
                    let high = self.high;
                    let low = self.low;
                    self.open = open;
                    self.close = close;
                    self.high = high;
                    self.low = low;
                }
            }

        }
    }
    /// Creates a new `candles` instance that is open and has not yet closed.
    ///
    /// # Arguments
    ///
    /// - `symbol`: The trading symbol of the asset.
    /// - `open`: The opening price.
    /// - `volume`: The trading volume.
    /// - `time`: The opening time as a Unix timestamp.
    ///
    pub fn new(symbol: Symbol, open: f64, volume: f64, time: String, resolution: Resolution) -> Self {
        Self {
            symbol,
            high: open,
            low: open,
            open,
            close: open,
            volume,
            range: 0.0,
            time,
            is_closed: false,
            resolution
        }
    }

    /// The actual candle object time, not adjusted for close etc, this is used when drawing the candle on charts.
    pub fn time_utc(&self) -> DateTime<chrono::Utc> {
        DateTime::from_str(&self.time).unwrap()
    }

    /// The actual candle object time, not adjusted for close etc, this is used when drawing the candle on charts.
    pub fn time_local(&self, time_zone: &Tz) -> DateTime<FixedOffset> {
        time_local_from_str(time_zone, &self.time)
    }

    /// Creates a new `candles` instance representing a completed (closed) trading period.
    ///
    /// # Arguments
    ///
    /// - `symbol`: The trading symbol of the asset.
    /// - `high`: The highest price during the candles's time.
    /// - `low`: The lowest price during the candles's time.
    /// - `open`: The opening price.
    /// - `close`: The closing price.
    /// - `volume`: The trading volume.
    /// - `time`: The opening time as a Unix timestamp.
    pub fn from_closed(symbol: Symbol, high: f64, low: f64, open: f64, close: f64, volume: f64, time: DateTime<chrono::Utc>, resolution: Resolution) -> Self {
        Self {
            symbol,
            high,
            low,
            open,
            close,
            volume,
            range: high - low,
            time: time.to_string(),
            is_closed: true,
            resolution
        }
    }

    /// Updates the candles with new price and volume information. Typically used
    /// during the trading period before the candles closes.
    ///
    /// # Arguments
    ///
    /// - `price`: The latest price.
    /// - `volume`: The additional volume since the last update.
    /// - `is_closed`: Indicates whether this update should close the candles.
    ///
    pub fn update(&mut self, price: f64, volume: f64, is_closed: bool) {
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.volume += volume;
        self.range = self.high - self.low;

        if is_closed {
            self.is_closed = true;
        }
    }
}

impl fmt::Display for Candle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?},{},{},{},{},{},{},{},{},{}",
            self.symbol, self.resolution, self.high, self.low, self.open, self.close, self.volume, self.range, self.time, self.is_closed
        )
    }
}

impl fmt::Debug for Candle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Candle {{ resolution {}, symbol: {:?}, high: {}, low: {}, open: {}, close: {}, volume: {}, range: {}, time: {}, is_closed: {} }}",
            self.resolution, self.symbol, self.high, self.low, self.open, self.close, self.volume, self.range, self.time, self.is_closed
        )
    }
}
















