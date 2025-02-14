use std::fmt;
use serde_derive::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use std::str::FromStr;
use chrono_tz::{America, Tz};
use chrono_tz::Tz::UTC;
use crate::apis::rithmic::rithmic_systems::RithmicSystem;
use crate::messages::data_server_messaging::FundForgeError;

#[derive(Serialize, Deserialize, Clone, Eq, Serialize_rkyv, Deserialize_rkyv,
    Archive, PartialEq, Debug, Hash, PartialOrd, Ord, Copy)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub enum Brokerage {
    Test, //DO NOT CHANGE ORDER
    Rithmic(RithmicSystem),
    Bitget,
    Oanda
}

impl Brokerage {
    pub fn timezone(&self) -> Tz {
        match self {
            Brokerage::Test => UTC,
            Brokerage::Rithmic(_) => America::Chicago,
            Brokerage::Bitget => UTC,
            Brokerage::Oanda => UTC,
        }
    }
}

impl fmt::Display for Brokerage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Brokerage::Test => "Test".to_string(),
            Brokerage::Rithmic(system) => format!("Rithmic {}", system.to_string()),
            Brokerage::Bitget => "Bitget".to_string(),
            Brokerage::Oanda => "Oanda".to_string(),
        };
        write!(f, "{}", s)
    }
}

impl FromStr for Brokerage {
    type Err = FundForgeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "Test" {
            Ok(Brokerage::Test)
        } else if s.starts_with("Rithmic") {
            let system_name = s.trim_start_matches("Rithmic ");
            if let Some(system) = RithmicSystem::from_string(system_name) {
                Ok(Brokerage::Rithmic(system))
            } else {
                Err(FundForgeError::ClientSideErrorDebug(format!(
                    "Unknown RithmicSystem string: {}",
                    system_name
                )))
            }
        } else if s == "Bitget" {
            Ok(Brokerage::Bitget)

        } else if "Oanda" == s {
            Ok(Brokerage::Oanda)
        }else {
            Err(FundForgeError::ClientSideErrorDebug(format!(
                "Invalid brokerage string: {}",
                s
            )))
        }
    }
}

