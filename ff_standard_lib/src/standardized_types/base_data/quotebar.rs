use std::fmt;
use std::fmt::{Debug, Display};
use std::str::FromStr;
use chrono::{DateTime, FixedOffset};
use chrono_tz::Tz;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use crate::apis::vendor::DataVendor;
use crate::standardized_types::enums::Resolution;
use crate::helpers::converters::time_local_from_str;
use crate::standardized_types::subscriptions::Symbol;

/// Represents a single quote bar in a financial chart, commonly used
/// in the financial technical analysis of price patterns.
///
/// # Fields
///
/// - `symbol`: The trading symbol of the asset.
/// - `bid_high`: The highest bid price.
/// - `bid_low`: The lowest bid price.
/// - `bid_open`: The opening bid price.
/// - `bid_close`: The closing bid price.
/// - `ask_high`: The highest ask price.
/// - `ask_low`: The lowest ask price.
/// - `ask_open`: The opening ask price.
/// - `ask_close`: The closing ask price.
/// - `volume`: The trading volume.
/// - `range`: The difference between the high and low prices.
/// - `time`: The opening time of the quote bar as a Unix timestamp.
/// - `spread`: The difference between the highest ask price and the lowest bid price.
/// - `is_closed`: Indicates whether the quote bar is closed.
#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq)]
#[archive(
compare(PartialEq),
check_bytes,
)]
#[archive_attr(derive(Debug))]
pub struct QuoteBar {
    pub symbol: Symbol,
    pub bid_high: f64,
    pub bid_low: f64,
    pub bid_open: f64,
    pub bid_close: f64,
    pub ask_high: f64,
    pub ask_low: f64,
    pub ask_open: f64,
    pub ask_close: f64,
    pub volume: f64,
    pub range: f64,
    pub time: String,
    pub spread: f64,
    pub is_closed: bool,
    pub resolution: Resolution
}

impl QuoteBar {
    /// Creates a new `QuoteBar` instance that is open and has not yet closed.
    ///
    /// # Properties
    ///
    /// - `symbol`: The trading symbol of the asset.
    /// - `high`: The highest price.
    /// - `low`: The lowest price.
    /// - `open`: The opening price.
    /// - `close`: The closing price.
    /// - `bid_high`: The highest bid price.
    /// - `bid_low`: The lowest bid price.
    /// - `bid_open`: The opening bid price.
    /// - `bid_close`: The closing bid price.
    /// - `ask_high`: The highest ask price.
    /// - `ask_low`: The lowest ask price.
    /// - `ask_open`: The opening ask price.
    /// - `ask_close`: The closing ask price.
    /// - `volume`: The trading volume.
    /// - `range`: The difference between the high and low prices.
    /// - `time`: The opening time of the quote bar as a Unix timestamp.
    /// - `spread`: The difference between the highest ask price and the lowest bid price.
    /// - `is_closed`: Indicates whether the quote bar is closed.
    /// - `data_vendor`: The data vendor that provided the quote bar.
    /// - `resolution`: The resolution of the quote bar.
    pub fn new(symbol: Symbol, bid_open: f64, ask_open: f64, volume: f64, time: String, resolution: Resolution) -> Self {
        Self {
            symbol,
            bid_high: bid_open,
            bid_low: bid_open,
            bid_open,
            bid_close: bid_open,
            ask_high: ask_open,
            ask_low: ask_open,
            ask_open,
            ask_close: ask_open,
            volume,
            range: 0.0,
            time,
            spread: ask_open - bid_open,
            is_closed: false,
            resolution
        }
    }

    /// Creates a new `QuoteBar` instance representing a completed (closed) trading period.
    ///
    /// # Arguments
    ///
    /// - `symbol`: The trading symbol of the asset.
    /// - `bid_high`: The highest bid price during the quote bar's time.
    /// - `bid_low`: The lowest bid price during the quote bar's time.
    /// - `bid_open`: The opening bid price.
    /// - `bid_close`: The closing bid price.
    /// - `ask_high`: The highest ask price during the quote bar's time.
    /// - `ask_low`: The lowest ask price during the quote bar's time.
    /// - `ask_open`: The opening ask price.
    /// - `ask_close`: The closing ask price.
    /// - `volume`: The trading volume.
    /// - `time`: The opening time as a Unix timestamp.
    /// - `data_vendor`: The data vendor that provided the quote bar.
    pub fn from_closed(symbol: Symbol, bid_high: f64, bid_low: f64, bid_open: f64, bid_close: f64, ask_high: f64, ask_low: f64, ask_open: f64, ask_close: f64, volume: f64, time: DateTime<chrono::Utc>, resolution: Resolution,data_vendor: DataVendor) -> Self {
        Self {
            symbol,
            bid_high,
            bid_low,
            bid_open,
            bid_close,
            ask_high,
            ask_low,
            ask_open,
            ask_close,
            volume,
            range: (ask_high + bid_high) - (ask_low + bid_low),
            time: time.to_string(),
            spread: ask_high - bid_low,
            is_closed: true,
            resolution
        }
    }

    pub fn time_utc(&self) -> DateTime<chrono::Utc> {
        DateTime::from_str(&self.time).unwrap()
    }

    pub fn time_local(&self, time_zone: &Tz) -> DateTime<FixedOffset> {
        time_local_from_str(time_zone, &self.time)
    }
}

impl Display for QuoteBar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.resolution, self.symbol, self.bid_high, self.bid_low, self.bid_open, self.bid_close, self.ask_high, self.ask_low, self.ask_open, self.ask_close, self.volume, self.range, self.spread, self.time, self.is_closed
        )
    }
}

impl Debug for QuoteBar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "QuoteBar {{ resolution: {}, symbol: {:?}, bid_high: {}, bid_low: {}, bid_open: {}, bid_close: {}, ask_high: {}, ask_low: {}, ask_open: {}, ask_close: {}, volume: {}, range: {}, spread: {}, time: {}, is_closed: {} }}",
            self.resolution , self.symbol, self.bid_high, self.bid_low, self.bid_open, self.bid_close, self.ask_high, self.ask_low, self.ask_open, self.ask_close, self.volume, self.range, self.spread, self.time, self.is_closed
        )
    }
}





