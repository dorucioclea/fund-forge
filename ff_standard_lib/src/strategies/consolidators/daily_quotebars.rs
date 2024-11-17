use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc, Weekday};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::messages::data_server_messaging::FundForgeError;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::base_data::quotebar::QuoteBar;
use crate::standardized_types::base_data::traits::BaseData;
use crate::standardized_types::enums::MarketType;
use crate::standardized_types::market_hours::{DaySession, TradingHours};
use crate::standardized_types::new_types::Price;
use crate::standardized_types::resolution::Resolution;
use crate::standardized_types::subscriptions::{CandleType, DataSubscription};
use crate::strategies::consolidators::consolidator_enum::ConsolidatedData;

pub struct DailyQuoteConsolidator {
    current_data: Option<BaseDataEnum>,
    pub(crate) subscription: DataSubscription,
    decimal_accuracy: u32,
    tick_size: Decimal,
    last_ask_close: Option<Price>,
    last_bid_close: Option<Price>,
    fill_forward: bool,
    market_type: MarketType,
    last_bar_open: DateTime<Utc>,
    trading_hours: TradingHours,
    days_per_bar: i64,
    current_bar_start_day: Option<DateTime<Utc>>,
}
#[allow(dead_code)]
impl DailyQuoteConsolidator {
    pub(crate) async fn new(
        subscription: DataSubscription,
        fill_forward: bool,
        decimal_accuracy: u32,
        tick_size: Decimal,
        trading_hours: TradingHours,
    ) -> Result<Self, FundForgeError> {
        println!("Creating Daily Quote Consolidator For: {}", subscription);

        if subscription.base_data_type == BaseDataType::Fundamentals
            || subscription.base_data_type == BaseDataType::Ticks
            || subscription.base_data_type == BaseDataType::Candles {
            return Err(FundForgeError::ClientSideErrorDebug(format!(
                "{} is an Invalid base data type for DailyQuoteConsolidator",
                subscription.base_data_type
            )));
        }

        let days_per_bar = match subscription.resolution {
            Resolution::Days(days) => days as i64,
            _ => return Err(FundForgeError::ClientSideErrorDebug(
                "DailyQuoteConsolidator requires Resolution::Daily".to_string()
            )),
        };

        let market_type = subscription.symbol.market_type.clone();

        Ok(DailyQuoteConsolidator {
            current_data: None,
            market_type,
            subscription,
            decimal_accuracy,
            tick_size,
            last_ask_close: None,
            last_bid_close: None,
            fill_forward,
            last_bar_open: DateTime::<Utc>::MIN_UTC,
            trading_hours,
            days_per_bar,
            current_bar_start_day: None,
        })
    }

    // Reuse all the session management methods from DailyConsolidator
    fn get_session_for_day(&self, weekday: Weekday) -> &DaySession {
        match weekday {
            Weekday::Mon => &self.trading_hours.monday,
            Weekday::Tue => &self.trading_hours.tuesday,
            Weekday::Wed => &self.trading_hours.wednesday,
            Weekday::Thu => &self.trading_hours.thursday,
            Weekday::Fri => &self.trading_hours.friday,
            Weekday::Sat => &self.trading_hours.saturday,
            Weekday::Sun => &self.trading_hours.sunday,
        }
    }

    fn get_next_market_open(&self, from_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // Same implementation as DailyConsolidator
        let mut check_time = from_time.with_timezone(&self.trading_hours.timezone);

        for _ in 0..14 {
            let current_session = self.get_session_for_day(check_time.weekday());

            if let Some(open_time) = current_session.open {
                let current_time = check_time.time();
                let market_datetime = if current_time >= open_time {
                    check_time.date_naive().succ_opt().unwrap()
                        .and_hms_opt(open_time.hour(), open_time.minute(), open_time.second())
                        .unwrap()
                } else {
                    check_time.date_naive()
                        .and_hms_opt(open_time.hour(), open_time.minute(), open_time.second())
                        .unwrap()
                };

                if let Some(tz_datetime) = self.trading_hours.timezone.from_local_datetime(&market_datetime).latest() {
                    return Some(tz_datetime.with_timezone(&Utc));
                }
            }

            check_time = check_time.date_naive().succ_opt().unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(self.trading_hours.timezone)
                .unwrap();
        }

        None
    }

    // Reuse bar timing logic from DailyConsolidator
    fn should_start_new_bar(&self, time: DateTime<Utc>) -> bool {
        if self.current_bar_start_day.is_none() {
            return true;
        }

        let start_day = self.current_bar_start_day.unwrap();
        let days_elapsed = (time - start_day).num_days();

        days_elapsed >= self.days_per_bar
    }

    fn is_session_end(&self, time: DateTime<Utc>) -> bool {
        let market_time = time.with_timezone(&self.trading_hours.timezone);
        let current_session = self.get_session_for_day(market_time.weekday());

        if let Some(close_time) = current_session.close {
            // If there's a close time, check if we've reached it
            return market_time.time() >= close_time;
        }

        // For sessions without explicit close time, we need to find the next trading session
        let mut check_time = market_time;
        let mut found_next_session = false;

        // Look through next 7 days to find next session
        for _ in 0..7 {
            check_time = check_time.date_naive().succ_opt().unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_local_timezone(self.trading_hours.timezone)
                .unwrap();

            let next_session = self.get_session_for_day(check_time.weekday());

            // First non-empty session we find is our boundary
            if next_session.open.is_some() {
                found_next_session = true;

                // Compare trading hours
                match (current_session.open, next_session.open) {
                    (Some(curr_open), Some(next_open)) => {
                        // Different open times indicate a session boundary
                        if curr_open != next_open {
                            return true;
                        }
                        // Same open time, this is a continuation of the same session
                        break;
                    }
                    _ => return true, // Trading hours change
                }
            }
        }

        // If we couldn't find any future sessions, this isn't a session end
        if !found_next_session {
            return false;
        }

        // If we got here, we found future sessions with the same trading hours
        false
    }

    fn fill_forward(&mut self, time: DateTime<Utc>) {
        if self.fill_forward {
            match self.subscription.base_data_type {
                BaseDataType::QuoteBars => {
                    if let (Some(last_bid_close), Some(last_ask_close)) = (self.last_bid_close, self.last_ask_close) {
                        let time = if let Some(next_open) = self.get_next_market_open(time) {
                            next_open
                        } else {
                            time
                        };

                        if time == self.last_bar_open {
                            return;
                        }

                        self.last_bar_open = time.clone();
                        let spread = self.market_type.round_price(
                            last_ask_close - last_bid_close,
                            self.tick_size,
                            self.decimal_accuracy
                        );

                        self.current_data = Some(BaseDataEnum::QuoteBar(QuoteBar {
                            symbol: self.subscription.symbol.clone(),
                            ask_open: last_ask_close,
                            ask_high: last_ask_close,
                            ask_low: last_ask_close,
                            ask_close: last_ask_close,
                            bid_open: last_bid_close,
                            bid_high: last_bid_close,
                            bid_low: last_bid_close,
                            bid_close: last_bid_close,
                            volume: dec!(0.0),
                            ask_volume: dec!(0.0),
                            bid_volume: dec!(0.0),
                            time: time.to_string(),
                            resolution: self.subscription.resolution.clone(),
                            is_closed: false,
                            range: dec!(0.0),
                            spread,
                            candle_type: CandleType::CandleStick,
                        }));
                    }
                }
                _ => {}
            }
        }
    }

    fn update_quote_bars(&mut self, base_data: &BaseDataEnum) -> ConsolidatedData {
        if !self.trading_hours.is_market_open(base_data.time_utc()) {
            return ConsolidatedData::with_open(base_data.clone());
        }

        if self.current_data.is_none() {
            let data = self.new_quote_bar(base_data);
            self.current_bar_start_day = Some(base_data.time_utc());
            self.current_data = Some(BaseDataEnum::QuoteBar(data.clone()));
            return ConsolidatedData::with_open(BaseDataEnum::QuoteBar(data));
        }

        let time = base_data.time_utc();

        // Check time ordering before any other operations
        if let Some(current_bar) = &self.current_data {
            if time < current_bar.time_utc() {
                return ConsolidatedData::with_open(base_data.clone());
            }
        }

        // Evaluate session end condition before taking mutable borrow
        let should_close = self.is_session_end(time) || self.should_start_new_bar(time);

        if should_close {
            // Now we can safely handle the closing of the bar
            if let Some(current_bar) = self.current_data.as_mut() {
                let mut consolidated_bar = current_bar.clone();
                consolidated_bar.set_is_closed(true);

                // Store last prices before we create new bar
                match &consolidated_bar {
                    BaseDataEnum::QuoteBar(quote_bar) => {
                        self.last_ask_close = Some(quote_bar.ask_close.clone());
                        self.last_bid_close = Some(quote_bar.bid_close.clone());
                    }
                    _ => {}
                }

                let new_bar = self.new_quote_bar(base_data);
                self.current_bar_start_day = Some(time);
                self.current_data = Some(BaseDataEnum::QuoteBar(new_bar.clone()));

                return ConsolidatedData::with_closed(
                    BaseDataEnum::QuoteBar(new_bar),
                    consolidated_bar
                );
            }
        }

        // Update existing bar
        if let Some(current_bar) = self.current_data.as_mut() {
            match current_bar {
                BaseDataEnum::QuoteBar(quote_bar) => match base_data {
                    BaseDataEnum::Quote(quote) => {
                        quote_bar.ask_high = quote_bar.ask_high.max(quote.ask);
                        quote_bar.ask_low = quote_bar.ask_low.min(quote.ask);
                        quote_bar.bid_high = quote_bar.bid_high.max(quote.bid);
                        quote_bar.bid_low = quote_bar.bid_low.min(quote.bid);
                        quote_bar.ask_close = quote.ask;
                        quote_bar.bid_close = quote.bid;
                        quote_bar.volume += quote.ask_volume + quote.bid_volume;
                        quote_bar.ask_volume += quote.ask_volume;
                        quote_bar.bid_volume += quote.bid_volume;
                        quote_bar.range = self.market_type.round_price(
                            quote_bar.ask_high - quote_bar.bid_low,
                            self.tick_size,
                            self.decimal_accuracy,
                        );
                        quote_bar.spread = self.market_type.round_price(
                            quote_bar.ask_close - quote_bar.bid_close,
                            self.tick_size,
                            self.decimal_accuracy,
                        );
                        ConsolidatedData::with_open(base_data.clone())
                    }
                    BaseDataEnum::QuoteBar(new_quote_bar) => {
                        quote_bar.ask_high = quote_bar.ask_high.max(new_quote_bar.ask_high);
                        quote_bar.ask_low = quote_bar.ask_low.min(new_quote_bar.ask_low);
                        quote_bar.bid_high = quote_bar.bid_high.max(new_quote_bar.bid_high);
                        quote_bar.bid_low = quote_bar.bid_low.min(new_quote_bar.bid_low);
                        quote_bar.ask_close = new_quote_bar.ask_close;
                        quote_bar.bid_close = new_quote_bar.bid_close;
                        quote_bar.volume += new_quote_bar.volume;
                        quote_bar.ask_volume += new_quote_bar.ask_volume;
                        quote_bar.bid_volume += new_quote_bar.bid_volume;
                        quote_bar.range = self.market_type.round_price(
                            quote_bar.ask_high - quote_bar.bid_low,
                            self.tick_size,
                            self.decimal_accuracy,
                        );
                        quote_bar.spread = self.market_type.round_price(
                            quote_bar.ask_close - quote_bar.bid_close,
                            self.tick_size,
                            self.decimal_accuracy,
                        );
                        ConsolidatedData::with_open(base_data.clone())
                    }
                    _ => panic!(
                        "Invalid base data type for QuoteBar consolidator: {}",
                        base_data.base_data_type()
                    ),
                },
                _ => panic!(
                    "Invalid base data type for QuoteBar consolidator: {}",
                    base_data.base_data_type()
                ),
            }
        } else {
            panic!(
                "Invalid base data type for QuoteBar consolidator: {}",
                base_data.base_data_type()
            );
        }
    }

    fn new_quote_bar(&mut self, new_data: &BaseDataEnum) -> QuoteBar {
        let time = if let Some(next_open) = self.get_next_market_open(new_data.time_utc()) {
            next_open
        } else {
            new_data.time_utc()
        };

        self.last_bar_open = time;

        match new_data {
            BaseDataEnum::Quote(quote) => {
                QuoteBar::new(
                    self.subscription.symbol.clone(),
                    quote.bid,
                    quote.ask,
                    quote.bid_volume + quote.ask_volume,
                    quote.ask_volume,
                    quote.bid_volume,
                    time.to_string(),
                    self.subscription.resolution.clone(),
                    CandleType::CandleStick,
                )
            }
            BaseDataEnum::QuoteBar(quote_bar) => {
                let mut consolidated_bar = quote_bar.clone();
                consolidated_bar.is_closed = false;
                consolidated_bar.resolution = self.subscription.resolution.clone();
                consolidated_bar.time = time.to_string();
                consolidated_bar
            }
            _ => panic!("Invalid base data type for QuoteBar consolidator"),
        }
    }

    pub fn update(&mut self, base_data: &BaseDataEnum) -> ConsolidatedData {
        match base_data.base_data_type() {
            BaseDataType::Quotes | BaseDataType::QuoteBars => self.update_quote_bars(base_data),
            _ => panic!("Only Quotes and QuoteBars are supported for daily quote consolidation"),
        }
    }
}