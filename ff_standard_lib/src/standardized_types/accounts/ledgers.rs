use async_std::task::block_on;
use chrono::{DateTime, Utc};
use crate::apis::brokerage::client_requests::ClientSideBrokerage;
use crate::apis::brokerage::Brokerage;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;
use crate::standardized_types::data_server_messaging::FundForgeError;
use crate::standardized_types::enums::{OrderSide, PositionSide};
use crate::standardized_types::subscriptions::{SymbolName};
use crate::standardized_types::time_slices::TimeSlice;
use crate::traits::bytes::Bytes;
use dashmap::DashMap;
use iso_currency::Currency;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use crate::helpers::decimal_calculators::{round_to_decimals, round_to_tick_size};
use crate::standardized_types::orders::orders::ProtectiveOrder;
use crate::standardized_types::Price;

// B
pub type AccountId = String;

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub enum AccountCurrency {
    AUD,
    USD,
    BTC,
}

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct SymbolInfo {
    symbol_name: SymbolName,
    pnl_currency: AccountCurrency,
    value_per_tick: f64,
    tick_size: f64
}

impl SymbolInfo {
    pub fn new(symbol_name: SymbolName,
               pnl_currency: AccountCurrency,
               value_per_tick: f64,
               tick_size: f64) -> Self {
        Self {
            symbol_name,
            pnl_currency,
            value_per_tick,
            tick_size
        }
    }
}

/// A ledger specific to the strategy which will ignore positions not related to the strategy but will update its balances relative to the actual account balances for live trading.
#[derive(Debug)]
pub struct Ledger {
    pub account_id: AccountId,
    pub brokerage: Brokerage,
    pub cash_value: f64,
    pub cash_available: f64,
    pub currency: AccountCurrency,
    pub cash_used: f64,
    pub positions: DashMap<SymbolName, Position>,
    pub positions_closed: DashMap<SymbolName, Vec<Position>>,
    pub positions_counter: DashMap<SymbolName, u64>,
    pub symbol_info: DashMap<SymbolName, SymbolInfo>,
    pub open_pnl: f64,
    pub booked_pnl: f64,
}
impl Ledger {
    pub fn new(account_info: AccountInfo) -> Self {
        let positions = account_info
            .positions
            .into_iter()
            .map(|position| (position.symbol_name.clone(), position))
            .collect();

        let ledger = Self {
            account_id: account_info.account_id,
            brokerage: account_info.brokerage,
            cash_value: account_info.cash_value,
            cash_available: account_info.cash_available,
            currency: account_info.currency,
            cash_used: account_info.cash_used,
            positions,
            positions_closed: DashMap::new(),
            positions_counter: DashMap::new(),
            symbol_info: DashMap::new(),
            open_pnl: 0.0,
            booked_pnl: 0.0,
        };
        ledger
    }

    pub fn update_brackets(&self, symbol_name: &SymbolName, brackets: Vec<ProtectiveOrder>) {
        if let Some(mut position) = self.positions.get_mut(symbol_name) {
            position.brackets = Some(brackets);
        }
    }

    pub fn print(&self) -> String {
        let mut total_trades: usize = 0;
        let mut losses: usize = 0;
        let mut wins: usize = 0;
        let mut win_pnl = 0.0;
        let mut loss_pnl = 0.0;
        let mut pnl = 0.0;
        for trades in self.positions_closed.iter() {
            total_trades += trades.value().len();
            for position in trades.value() {
                if position.booked_pnl > 0.0 {
                    wins += 1;
                    win_pnl += position.booked_pnl;
                } else {
                    loss_pnl += position.booked_pnl;
                    if position.booked_pnl < 0.0 {
                        losses += 1;
                    }
                }
                pnl += position.booked_pnl;
            }
        }

        let risk_reward = if losses > 0 {
            win_pnl / -loss_pnl// negate loss_pnl for correct calculation
        } else {
            0.0
        };

        // Calculate profit factor
        let profit_factor = if loss_pnl != 0.0 {
            win_pnl / -loss_pnl// negate loss_pnl
        } else {
            0.0
        };

        // Calculate win rate
        let win_rate = if total_trades > 0 {
            wins as f64 / total_trades as f64 * 100.0
        } else {
            0.0
        };

        let break_even = total_trades - wins - losses;
        format!("Brokerage: {}, Account: {}, Balance: {:.2}, Win Rate: {:.2}%, Risk Reward: {:.2}, Profit Factor {:.2}, Total profit: {:.2}, Total Wins: {}, Total Losses: {}, Break Even: {}, Total Trades: {}",
                self.brokerage, self.account_id, self.cash_value, win_rate, risk_reward, profit_factor, pnl, wins, losses,  break_even, total_trades) //todo, check against self.pnl


    }

    pub fn paper_account_init(
        account_id: AccountId,
        brokerage: Brokerage,
        cash_value: f64,
        currency: AccountCurrency,
    ) -> Self {
        let account = Self {
            account_id,
            brokerage,
            cash_value,
            cash_available: cash_value,
            currency,
            cash_used: 0.0,
            positions: DashMap::new(),
            positions_closed: DashMap::new(),
            positions_counter: DashMap::new(),
            symbol_info: DashMap::new(),
            open_pnl: 0.0,
            booked_pnl: 0.0,
        };
        account
    }

    pub async fn generate_id(&self, symbol_name: &SymbolName, time: DateTime<Utc>, side: PositionSide) -> PositionId {
        self.positions_counter.entry(symbol_name.clone())
            .and_modify(|value| *value += 1)  // Increment the value if the key exists
            .or_insert(1);

        format!("{}-{}-{}-{}-{}-{}", self.brokerage, self.account_id, symbol_name, time.timestamp(), self.positions_counter.get(symbol_name).unwrap().value().clone(), side)
    }

    pub async fn update_or_create_paper_position(
        &mut self,
        symbol_name: &SymbolName,
        quantity: u64,
        price: Price,
        side: OrderSide,
        time: &DateTime<Utc>,
        brackets: Option<Vec<ProtectiveOrder>>
    ) -> Result<(), FundForgeError>
    {
        // Check if there's an existing position for the given symbol
        if self.positions.contains_key(symbol_name) {
            let (_, mut existing_position) = self.positions.remove(symbol_name).unwrap();
            let is_reducing = (existing_position.side == PositionSide::Long && side == OrderSide::Sell) || (existing_position.side == PositionSide::Short && side == OrderSide::Buy);
            if is_reducing {
                let pnl = existing_position.reduce_position_size(price, quantity, time);
                if let Some(new_brackets) = brackets {
                    existing_position.brackets = Some(new_brackets);
                }
                self.booked_pnl += pnl;
                self.cash_available += pnl;
                self.cash_used -= self.brokerage.margin_required_historical(symbol_name.clone(), quantity).await;
                self.cash_value = self.cash_available + self.cash_used;
                // Update PnL: if the entire position is closed, set PnL to 0
                if !existing_position.is_closed {
                    self.positions.insert(symbol_name.clone(), existing_position);
                } else {
                    if !self.positions_closed.contains_key(symbol_name) {
                        self.positions_closed.insert(symbol_name.clone(), vec![existing_position]);
                    } else {
                        if let Some(mut positions) = self.positions_closed.get_mut(symbol_name) {
                            positions.value_mut().push(existing_position);
                        }
                    }
                }
                Ok(())
            } else {
                // Calculate the margin required for adding more to the short position
                let margin_required = self.brokerage.margin_required_historical(symbol_name.clone(), quantity).await;
                // Check if the available cash is sufficient to cover the margin
                if self.cash_available < margin_required {
                    return Err(FundForgeError::ClientSideErrorDebug("Insufficient funds".to_string()));
                }

                let open_pnl = existing_position.open_pnl;
                existing_position.add_to_position(price, quantity, time);
                if let Some(new_brackets) = brackets {
                    existing_position.brackets = Some(new_brackets);
                }
                // Deduct margin from cash available
                self.open_pnl -= open_pnl;
                self.open_pnl += existing_position.open_pnl;
                self.cash_available -= margin_required;
                self.cash_used += margin_required;
                self.cash_value = self.cash_available + self.cash_used;
                self.positions.insert(symbol_name.clone(), existing_position);
                Ok(())
            }
        } else {
            // No existing position, create a new one
            let margin_required = self.brokerage.margin_required_historical(symbol_name.clone(), quantity).await;
            // Check if the available cash is sufficient to cover the margin
            if self.cash_available < margin_required {
                return Err(FundForgeError::ClientSideErrorDebug("Insufficient funds".to_string()));
            }

            if !self.symbol_info.contains_key(symbol_name) {
                let symbol_info = match self.brokerage.symbol_info(symbol_name.clone()).await {
                    Ok(info) => info,
                    Err(_) => return Err(FundForgeError::ClientSideErrorDebug(format!("No Symbol info for: {}", symbol_name)))
                };
                self.symbol_info.insert(symbol_name.clone(),symbol_info);
            }

            // Determine the side of the position based on the order side
            let side = match side {
                OrderSide::Buy => PositionSide::Long,
                OrderSide::Sell => PositionSide::Short
            };

            // Create a new position with the given details
            let position = Position::enter(
                symbol_name.clone(),
                self.brokerage.clone(),
                side,
                quantity,
                price,
                self.generate_id(&symbol_name, time.clone(), side).await,
                self.symbol_info.get(symbol_name).unwrap().value().clone(),
                self.currency.clone(),
                brackets
            );

            // Insert the new position into the positions map
            self.positions.insert(position.symbol_name.clone(), position);

            // Adjust the account cash to reflect the new position's margin requirement
            self.cash_available -= margin_required;
            self.cash_used += margin_required;
            self.cash_value = self.cash_available + self.cash_used;
            Ok(())
        }
    }

    pub async fn exit_position_paper(&mut self, symbol_name: &SymbolName) {
        if !self.positions.contains_key(symbol_name) {
            return;
        }
        let (_, mut old_position) = self.positions.remove(symbol_name).unwrap();
        let margin_freed = self
            .brokerage
            .margin_required_historical(symbol_name.clone(), old_position.quantity)
            .await;

        old_position.is_closed = true;
        old_position.booked_pnl += old_position.open_pnl;
        self.booked_pnl += old_position.open_pnl;
        self.cash_available += margin_freed + old_position.open_pnl;
        self.cash_used -= margin_freed;
        self.cash_value +=  old_position.open_pnl;
        old_position.open_pnl = 0.0;
        if !self.positions_closed.contains_key(&old_position.symbol_name) {
            self.positions_closed.insert(old_position.symbol_name.clone(), vec![old_position]);
        } else {
            self.positions_closed.get_mut(&old_position.symbol_name).unwrap().value_mut().push(old_position);
        }
    }

    pub async fn account_init(account_id: AccountId, brokerage: Brokerage) -> Self {
        match brokerage.init_ledger(account_id).await {
            Ok(ledger) => ledger,
            Err(e) => panic!("Failed to initialize account: {:?}", e),
        }
    }

    pub async fn is_long(&self, symbol_name: &SymbolName) -> bool {
        if let Some(position) = self.positions.get(symbol_name) {
            if position.side == PositionSide::Long {
                return true;
            }
        }
        false
    }

    pub async fn is_short(&self, symbol_name: &SymbolName) -> bool {
        if let Some(position) = self.positions.get(symbol_name) {
            if position.side == PositionSide::Short {
                return true;
            }
        }
        false
    }

    pub async fn is_flat(&self, symbol_name: &SymbolName) -> bool {
        if let Some(_) = self.positions.get(symbol_name) {
            false
        } else {
            true
        }
    }

    pub async fn on_data_update(&mut self, time_slice: TimeSlice, time: &DateTime<Utc>) {
        let mut open_pnl = 0.0;
        let mut booked_pnl = 0.0;
        for base_data_enum in time_slice {
            let data_symbol_name = &base_data_enum.symbol().name;
            if let Some(mut position) = self.positions.get_mut(data_symbol_name) {
                booked_pnl += position.update_base_data(&base_data_enum, time);
                open_pnl += position.open_pnl;

                if position.is_closed {
                    // Move the position to the closed positions map
                   let (symbol_name, position) = self.positions.remove(data_symbol_name).unwrap();
                    self.positions_closed
                        .entry(symbol_name)
                        .or_insert_with(Vec::new)
                        .push(position);
                }
            }
        }
        self.open_pnl = open_pnl;
        self.booked_pnl += booked_pnl;
    }
}

pub type PositionId = String;
#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct Position {
    pub symbol_name: SymbolName,
    pub brokerage: Brokerage,
    pub side: PositionSide,
    pub quantity: u64,
    pub average_price: Price,
    pub open_pnl: f64,
    pub booked_pnl: f64,
    pub highest_recoded_price: Price,
    pub lowest_recoded_price: Price,
    pub is_closed: bool,
    pub id: PositionId,
    pub symbol_info: SymbolInfo,
    account_currency: AccountCurrency,
    brackets: Option<Vec<ProtectiveOrder>>
}
//todo make it so stop loss and take profit can be attached to positions, then instead of updating in the market handler, those orders update in the position and auto cancel themselves when position closes
impl Position {
    pub fn enter(
        symbol_name: SymbolName,
        brokerage: Brokerage,
        side: PositionSide,
        quantity: u64,
        average_price: f64,
        id: PositionId,
        symbol_info: SymbolInfo,
        account_currency: AccountCurrency,
        brackets: Option<Vec<ProtectiveOrder>>
    ) -> Self {
        Self {
            symbol_name,
            brokerage,
            side,
            quantity,
            average_price,
            open_pnl: 0.0,
            booked_pnl: 0.0,
            highest_recoded_price: average_price,
            lowest_recoded_price: average_price,
            is_closed: false,
            id,
            symbol_info,
            account_currency,
            brackets
        }
    }

    pub fn reduce_position_size(&mut self, market_price: Price, quantity: u64, time: &DateTime<Utc>) -> f64 {
        let booked_pnl = calculate_pnl(&self.side, self.average_price, market_price, self.symbol_info.tick_size, self.symbol_info.value_per_tick, quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
        self.booked_pnl = booked_pnl;
        self.open_pnl -= booked_pnl;
        self.quantity -= quantity;
        self.is_closed = self.quantity <= 0;
        if self.is_closed {
            self.open_pnl = 0.0;
        }
        booked_pnl
    }

    pub fn add_to_position(&mut self, market_price: Price, quantity: u64, time: &DateTime<Utc>) {
        // If adding to the short position, calculate the new average price
        self.average_price =
            ((self.quantity as f64 * self.average_price) + (quantity as f64 * market_price))
                / (self.quantity + quantity) as f64;

        self.open_pnl =  calculate_pnl(&self.side, self.average_price, market_price, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);

        // Update the total quantity
        self.quantity += quantity;
    }

    pub fn update_base_data(&mut self, base_data: &BaseDataEnum, time: &DateTime<Utc>) -> f64 {
        if self.is_closed {
            return 0.0;
        }
        let market_price = match base_data {
            BaseDataEnum::Price(price) => {
                self.highest_recoded_price = self.highest_recoded_price.max(price.price);
                self.lowest_recoded_price = self.lowest_recoded_price.min(price.price);
                self.open_pnl = calculate_pnl(&self.side, self.average_price, price.price, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
                price.price
            }
            BaseDataEnum::Candle(candle) => {
                self.highest_recoded_price = self.highest_recoded_price.max(candle.high);
                self.lowest_recoded_price = self.lowest_recoded_price.min(candle.low);
                self.open_pnl = calculate_pnl(&self.side, self.average_price, candle.close, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
                candle.close
            }
            BaseDataEnum::QuoteBar(bar) => {
                match self.side {
                    PositionSide::Long => {
                        self.highest_recoded_price = self.highest_recoded_price.max(bar.ask_high);
                        self.lowest_recoded_price = self.lowest_recoded_price.min(bar.ask_low);
                        self.open_pnl = calculate_pnl(&self.side, self.average_price, bar.ask_close, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
                        bar.ask_close
                    }
                    PositionSide::Short => {
                        self.highest_recoded_price = self.highest_recoded_price.max(bar.bid_high);
                        self.lowest_recoded_price = self.lowest_recoded_price.min(bar.bid_low);
                        self.open_pnl = calculate_pnl(&self.side, self.average_price, bar.bid_close, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
                        bar.bid_close
                    }
                }

            }
            BaseDataEnum::Tick(tick) => {
                self.highest_recoded_price = self.highest_recoded_price.max(tick.price);
                self.lowest_recoded_price = self.lowest_recoded_price.min(tick.price);
                self.open_pnl = calculate_pnl(&self.side, self.average_price, tick.price, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);
                tick.price
            }
            BaseDataEnum::Quote(quote) => {
                match self.side {
                    PositionSide::Long => {
                        quote.ask
                    }
                    PositionSide::Short => {
                        quote.bid
                    }
                }
            }
            BaseDataEnum::Fundamental(_) => panic!("Fundamentals shouldnt be here")
        };
        self.highest_recoded_price = self.highest_recoded_price.max(market_price);
        self.lowest_recoded_price = self.lowest_recoded_price.min(market_price);
        self.open_pnl = calculate_pnl(&self.side, self.average_price, market_price, self.symbol_info.tick_size, self.symbol_info.value_per_tick, self.quantity, &self.symbol_info.pnl_currency, &self.account_currency, time);

        let mut bracket_triggered = false;
        if let Some(brackets) = &self.brackets {
            'bracket_loop: for bracket in brackets {
                match bracket {
                    ProtectiveOrder::TakeProfit { price } => match self.side {
                        PositionSide::Long => {
                            if market_price >= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                        PositionSide::Short => {
                            if market_price <= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                    },
                    ProtectiveOrder::StopLoss { price } => match self.side {
                        PositionSide::Long => {
                            if market_price <= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                        PositionSide::Short => {
                            if market_price >= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                    },
                    ProtectiveOrder::TrailingStopLoss { price, trail_value } => match self.side {
                        PositionSide::Long => {
                            if market_price <= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                        PositionSide::Short => {
                            if market_price >= *price {
                                bracket_triggered = true;
                                break 'bracket_loop;
                            } else {
                                bracket_triggered = false;
                            }
                        }
                    },
                }
            }
        }
        if bracket_triggered {
            let new_pnl = self.open_pnl;
            self.booked_pnl += new_pnl;
            self.open_pnl = 0.0;
            self.is_closed = true;
            return new_pnl;
        }
        0.0
    }
}


fn calculate_pnl(side: &PositionSide, entry_price: f64, market_price: f64, tick_size: f64, value_per_tick: f64, quantity: u64, base_currency: &AccountCurrency, account_currency: &AccountCurrency, time: &DateTime<Utc>) -> f64 {
    let pnl = match side {
        PositionSide::Long => round_to_tick_size(market_price - entry_price, tick_size) * (value_per_tick * quantity as f64),
        PositionSide::Short => round_to_tick_size(entry_price - market_price, tick_size) * (value_per_tick * quantity as f64),
    };

    // Placeholder for currency conversion if needed
    // let pnl = convert_currency(pnl, base_currency, account_currency, time);
    pnl
}

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct AccountInfo {
    pub account_id: AccountId,
    pub brokerage: Brokerage,
    pub cash_value: f64,
    pub cash_available: f64,
    pub currency: AccountCurrency,
    pub cash_used: f64,
    pub positions: Vec<Position>,
    pub positions_closed: Vec<Position>,
    pub is_hedging: bool,
}
impl Bytes<Self> for AccountInfo {
    fn from_bytes(archived: &[u8]) -> Result<AccountInfo, FundForgeError> {
        // If the archived bytes do not end with the delimiter, proceed as before
        match rkyv::from_bytes::<AccountInfo>(archived) {
            //Ignore this warning: Trait `Deserialize<ResponseType, SharedDeserializeMap>` is not implemented for `ArchivedRequestType` [E0277]
            Ok(response) => Ok(response),
            Err(e) => Err(FundForgeError::ClientSideErrorDebug(e.to_string())),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let vec = rkyv::to_bytes::<_, 256>(self).unwrap();
        vec.into()
    }
}
