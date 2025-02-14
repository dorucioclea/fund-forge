use chrono::{Duration, NaiveDate};
use chrono_tz::Australia;
use colored::Colorize;
use rust_decimal::Decimal;
use ff_standard_lib::standardized_types::base_data::base_data_enum::BaseDataEnum;
use ff_standard_lib::standardized_types::base_data::traits::BaseData;
use ff_standard_lib::standardized_types::enums::{MarketType, PrimarySubscription, StrategyMode};
use ff_standard_lib::strategies::strategy_events::{StrategyEvent};
use ff_standard_lib::standardized_types::subscriptions::{DataSubscription, SymbolCode, SymbolName};
use ff_standard_lib::strategies::fund_forge_strategy::FundForgeStrategy;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use ff_standard_lib::apis::rithmic::rithmic_systems::RithmicSystem;
use ff_standard_lib::product_maps::rithmic::maps::{get_exchange_by_symbol_name, get_futures_trading_hours};
use ff_standard_lib::standardized_types::accounts::{Account, AccountId, Currency};
use ff_standard_lib::standardized_types::base_data::base_data_type::BaseDataType;
use ff_standard_lib::standardized_types::broker_enum::Brokerage;
use ff_standard_lib::standardized_types::datavendor_enum::DataVendor;
use ff_standard_lib::standardized_types::orders::{OrderUpdateEvent};
use ff_standard_lib::standardized_types::position::PositionUpdateEvent;
use ff_standard_lib::standardized_types::resolution::Resolution;

// to launch on separate machine
#[tokio::main]
async fn main() {
    let (strategy_event_sender, strategy_event_receiver) = mpsc::channel(1000);

    let symbol_name = SymbolName::from("MES");
    let market_hours = get_futures_trading_hours(&symbol_name).unwrap();
    let exchange = get_exchange_by_symbol_name(&symbol_name).unwrap();
    let symbol_code = "MESZ4".to_string();
    let account_1 = Account::new(Brokerage::Rithmic(RithmicSystem::Apex), AccountId::from("APEX-3396-169"));
    let strategy = FundForgeStrategy::initialize(
        StrategyMode::Live,
        dec!(100000),
        Currency::USD,
        NaiveDate::from_ymd_opt(2019, 07, 1).unwrap().and_hms_opt(1, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2019, 07, 10).unwrap().and_hms_opt(23, 0, 0).unwrap(),
        Australia::Sydney,
        Duration::hours(1),
        vec![
            (Some(PrimarySubscription::new(Resolution::Ticks(1), BaseDataType::Ticks)), DataSubscription::new (
                symbol_name.clone(),
                DataVendor::Rithmic,
                Resolution::Seconds(3),
                BaseDataType::Candles,
                MarketType::Futures(exchange),
            ), None),
/*
            (Some(PrimarySubscription::new(Resolution::Minutes(1), BaseDataType::Candles)), DataSubscription::new (
                symbol_name.clone(),
                DataVendor::Rithmic,
                Resolution::Day,
                BaseDataType::Candles,
                MarketType::Futures(exchange),
            ), Some(market_hours.clone())),*/
        ],
        false,
        100,
        strategy_event_sender,
        core::time::Duration::from_millis(100),
        false,
        false,
        true,
        vec![account_1.clone()],
    ).await;

    on_data_received(strategy, strategy_event_receiver, symbol_name, symbol_code, account_1).await;
}


pub async fn on_data_received(
    strategy: FundForgeStrategy,
    mut event_receiver: mpsc::Receiver<StrategyEvent>,
    _symbol_name: SymbolName,
    symbol_code: SymbolCode,
    account_1: Account,
) {
    let mut exit_sent = false;
    let mut count = 1;
    let mut entry_order_id = "".to_string();
    let mut warmup_complete = false;
    'strategy_loop: while let Some(strategy_event) = event_receiver.recv().await {
        match strategy_event {
            StrategyEvent::TimeSlice(time_slice) => {
                for base_data in time_slice.iter() {
                    match base_data {
                        BaseDataEnum::Tick(tick) => {
                           /* let msg = format!("{} {} Tick: {}, {}, Aggressor: {}", tick.symbol.name, tick.time_local(strategy.time_zone()), tick.price, tick.volume, tick.aggressor);
                            match tick.aggressor {
                                Aggressor::Buy => {
                                    println!("{}", msg.as_str().bright_green());
                                },
                                Aggressor::Sell => {
                                    println!("{}", msg.as_str().bright_red());

                                },
                                Aggressor::None => {
                                    println!("{}", msg.as_str().cyan());
                                },
                            }*/
                        }
                        BaseDataEnum::Candle(candle) => {
                            // Place trades based on the AUD-CAD Heikin Ashi Candles
                            if candle.is_closed == true {
                                let msg = format!("{} {} {} Close: {}, {}", candle.symbol.name, candle.resolution, candle.candle_type, candle.close, candle.time_local(strategy.time_zone()));
                                if candle.close == candle.open {
                                    println!("{}", msg.as_str().blue())
                                } else {
                                    match candle.close > candle.open {
                                        true => println!("{}", msg.as_str().bright_green()),
                                        false => println!("{}", msg.as_str().bright_red()),
                                    }
                                }
                                if !warmup_complete {
                                    continue;
                                }
                                count += 1;
                                //LONG CONDITIONS
                                {
                                    if count == 5 {
                                        println!("Rithmic Order Test: Enter Long, Time {}", strategy.time_local());
                                        entry_order_id = strategy.enter_long(&candle.symbol.name, Some(symbol_code.clone()) ,&account_1, None, dec!(1), String::from("Enter Long")).await;
                                    }

                                    let open_pnl = strategy.pnl(&account_1, &symbol_code);
                                    let is_long = strategy.is_long(&account_1, &symbol_code);
                                    println!("Rithmic Order Test Long = {}, Pnl = {}", is_long, open_pnl);

                                    // Remove order_placed from exit condition
                                    if count == 10 && !exit_sent {
                                        exit_sent = true;
                                        strategy.cancel_orders_account(account_1.clone()).await;
                                        let position_size: Decimal = strategy.position_size(&account_1, &symbol_code);
                                        println!("Rithmic Order Test: Exit Long, Time {}: Size: {}", strategy.time_local(), position_size);
                                        let _exit_order_id = strategy.exit_long(&candle.symbol.name, Some(symbol_code.clone()), &account_1, None, position_size, String::from("Exit Long")).await;
                                    }
                                }

                                if count > 15 {
                                    //strategy.export_trades(&String::from("./trades exports"));
                                    let open_pnl = strategy.pnl(&account_1, &symbol_code);
                                    let is_long = strategy.is_long(&account_1, &symbol_code);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            StrategyEvent::ShutdownEvent(event) => {
                strategy.flatten_all_for(account_1).await;
                let msg = format!("{}",event);
                println!("{}", msg.as_str().bright_magenta());
                strategy.export_positions_to_csv(&String::from("./trades exports"));
                strategy.print_ledgers();
                //we should handle shutdown gracefully by first ending the strategy loop.
                break 'strategy_loop
            },
            StrategyEvent::PositionEvents(event) => {
                match event {
                    PositionUpdateEvent::PositionOpened { ..} => {}
                    PositionUpdateEvent::Increased { .. } => {}
                    PositionUpdateEvent::PositionReduced { .. } => {},
                    PositionUpdateEvent::PositionClosed { .. } => {
                        strategy.print_ledger(event.account())
                    },
                }
                let quantity = strategy.position_size(&account_1, &symbol_code);
                let msg = format!("{}, Time Local: {}", event, event.time_local(strategy.time_zone()));
                println!("{}", msg.as_str().purple());
                println!("Rithmic Order Test: Open Quantity: {}", quantity);
            }
            StrategyEvent::OrderEvents(event) => {
                let msg = format!("Rithmic Order Test: Order Event: {}, Time: {}", event, event.time_local(strategy.time_zone()));
                match event {
                    OrderUpdateEvent::OrderRejected { .. } | OrderUpdateEvent::OrderUpdateRejected { .. } => {
                        strategy.print_ledger(event.account());
                        println!("{}", msg.as_str().on_bright_magenta().on_bright_red())
                    },
                    _ =>  println!("{}", msg.as_str().bright_yellow())
                }
            }
            StrategyEvent::WarmUpComplete => {
                warmup_complete = true;
                println!("Rithmic Order Test: Warmup Complete");
            }
            _ => {}
        }
    }
    event_receiver.close();
}