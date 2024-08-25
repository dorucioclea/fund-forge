use std::fmt;
use std::fmt::{Debug, Display, Error, Formatter};
use rkyv::{AlignedVec, Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use rkyv::ser::Serializer;
use rkyv::ser::serializers::AllocSerializer;
use crate::apis::vendor::DataVendor;
use crate::consolidators::renko::RenkoParameters;
use crate::standardized_types::base_data::base_data_type::BaseDataType;
use crate::standardized_types::enums::{MarketType, Resolution};
use crate::helpers::converters::fund_forge_formatted_symbol_name;

pub type ExchangeCode = String;

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[archive(
// This will generate a PartialEq impl between our unarchived and archived
// types:
compare(PartialEq),
// bytecheck can be used to validate your data if you want. To use the safe
// API, you have to derive CheckBytes for the archived type:
check_bytes,
)]
#[archive_attr(derive(Debug))]
pub struct Symbol {
    pub name: String,
    pub market_type: MarketType,
    pub data_vendor: DataVendor,
}

impl Symbol {
    pub fn new(name: String, data_vendor: DataVendor,  market_type: MarketType) -> Self {
        Symbol {
            name,
            market_type,
            data_vendor,
        }
    }

    pub fn from_array_bytes(data: &Vec<u8>) -> Result<Vec<Symbol>, Error> {
        let archived_quotebars = match rkyv::check_archived_root::<Vec<Symbol>>(&data[..]){
            Ok(data) => data,
            Err(_) => {
                return Err(Error);
            },
        };

        // Assuming you want to work with the archived data directly, or you can deserialize it further
        Ok(archived_quotebars.deserialize(&mut rkyv::Infallible).unwrap())
    }

    /// Serializes a `Vec<Symbol>` into `AlignedVec`
    pub fn vec_to_aligned(symbols: Vec<Symbol>) -> AlignedVec {
        // Create a new serializer
        let mut serializer = AllocSerializer::<1024>::default();

        // Serialize the Vec<QuoteBar>
        serializer.serialize_value(&symbols).unwrap();

        // Get the serialized bytes
        let vec = serializer.into_serializer().into_inner();
        vec
    }

    /// Serializes a `Vec<Symbol>` into `Vec<u8>`
    /// This is the method used for quote bar request_response
    pub fn vec_to_bytes(symbols: Vec<Symbol>) -> Vec<u8> {
        // Get the serialized bytes
        let vec = Symbol::vec_to_aligned(symbols);
        vec.to_vec()
    }
}

#[derive(Debug, Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[archive(
    // This will generate a PartialEq impl between our unarchived and archived
    // types:
    compare(PartialEq),
    // bytecheck can be used to validate your data if you want. To use the safe
    // API, you have to derive CheckBytes for the archived type:
    check_bytes,
)]
#[archive_attr(derive(Debug))]
pub enum CandleType {
    Renko(RenkoParameters),
    HeikinAshi,
    CandleStick
}

impl Display for CandleType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CandleType::Renko(params) => {
                write!(f, "{}", format!("Renko: {}", params))
            },
            CandleType::HeikinAshi => {
                write!(f, "{}", "Heikin Ashi")
            },
            CandleType::CandleStick => {
                write!(f, "{}", "Candle Stick")
            }
        }
    }
}

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[archive(
// This will generate a PartialEq impl between our unarchived and archived
// types:
compare(PartialEq),
// bytecheck can be used to validate your data if you want. To use the safe
// API, you have to derive CheckBytes for the archived type:
check_bytes,
)]
#[archive_attr(derive(Debug))]
/// Subscription struct is used to define requests for `Vec<BaseDataEnum>` data.
/// The specific properties will define the request for the data.
///
/// # Properties
/// * `symbol` - The symbol of the subscription as a `String`.
/// * `data_vendor` - The data vendor of the subscription. [DataVendor](ff_strategy::vendors::DataVendor)
/// * `resolution` - The resolution of the subscription. [Resolution](ff_common_library::models::resolution::Resolution)
/// * `base_data_type` - The base data type of the subscription. [BaseDataType](crate::base_data::base_data_type::BaseDataType)
/// * `name` - The name of the subscription, this can be used to [`BaseDataType::Fundamental`](crate::base_data::base_data_type::BaseDataType) data. to give the subscription a name, this name can be used to sort and categorise the data in the strategy.
pub struct DataSubscription {
    pub symbol: Symbol,
    pub resolution: Resolution,
    pub base_data_type: BaseDataType,
    pub market_type: MarketType,
    pub candle_type: Option<CandleType>,
}

impl Display for DataSubscription {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.candle_type {
            Some(candle_type) => {
                write!(f, "{} {} {} {} {}: {}", self.symbol.name, self.symbol.data_vendor, self.base_data_type, self.resolution, self.market_type, candle_type)
            },
            None => {
                write!(f, "{} {} {} {} {}", self.symbol.name, self.symbol.data_vendor, self.base_data_type, self.resolution, self.market_type)
            }
        }
    }
}

impl DataSubscription {

    // we use this for any data that is represented by base data types
    pub fn new(symbol_name: String, data_vendor: DataVendor, resolution: Resolution, base_data_type: BaseDataType, market_type: MarketType) -> Self {
        let cleaned_symbol_name = fund_forge_formatted_symbol_name(&symbol_name);
        let symbol = Symbol::new(cleaned_symbol_name, data_vendor, market_type.clone());
        let candle_type = match base_data_type {
            BaseDataType::Candles => Some(CandleType::CandleStick),
            BaseDataType::QuoteBars => Some(CandleType::CandleStick),
            BaseDataType::Ticks => {
                match resolution {
                    Resolution::Ticks(number) => if number > 1 { Some(CandleType::CandleStick) } else { None },
                    _ => None
                }
            },
            _ => None
        };
        DataSubscription {
            symbol,
            resolution,
            base_data_type,
            market_type,
            candle_type
        }
    }

    /// We can use this to consolidate custom candle types which are not represented by the base data types
    pub fn new_custom(symbol_name: String, data_vendor: DataVendor, resolution: Resolution, base_data_type: BaseDataType, market_type: MarketType, candle_type: CandleType) -> Self {
        let cleaned_symbol_name = fund_forge_formatted_symbol_name(&symbol_name);
        let symbol = Symbol::new(cleaned_symbol_name, data_vendor, market_type.clone());

        DataSubscription {
            symbol,
            resolution,
            base_data_type,
            market_type,
            candle_type: Some(candle_type)
        }
    }
    
    pub fn from_base_data(symbol_name: String, data_vendor: DataVendor, resolution: Resolution, base_data_type: BaseDataType, market_type: MarketType, candle_type: Option<CandleType>) -> Self {
        let cleaned_symbol_name = fund_forge_formatted_symbol_name(&symbol_name);
        let symbol = Symbol::new(cleaned_symbol_name, data_vendor, market_type.clone());

        DataSubscription {
            symbol,
            resolution,
            base_data_type,
            market_type,
            candle_type
        }
    }

    /// Deserializes from `Vec<u8>` to `Vec<Subscription>`
    pub fn from_array_bytes(data: &Vec<u8>) -> Result<Vec<DataSubscription>, Error> {
        let archived_quotebars = match rkyv::check_archived_root::<Vec<DataSubscription>>(&data[..]){
            Ok(data) => data,
            Err(_) => {
                return Err(Error);
            },
        };

        // Assuming you want to work with the archived data directly, or you can deserialize it further
        Ok(archived_quotebars.deserialize(&mut rkyv::Infallible).unwrap())
    }

    /// Serializes a `Vec<DataSubscription>` into `AlignedVec`
    pub fn vec_to_aligned(subscriptions: Vec<DataSubscription>) -> AlignedVec {
        // Create a new serializer
        let mut serializer = AllocSerializer::<1024>::default();

        // Serialize the Vec<QuoteBar>
        serializer.serialize_value(&subscriptions).unwrap();

        // Get the serialized bytes
        let vec = serializer.into_serializer().into_inner();
        vec
    }

    /// Serializes a `Vec<DataSubscription>` into `Vec<u8>`
    /// This is the method used for quote bar request_response
    pub fn vec_to_bytes(subscriptions: Vec<DataSubscription>) -> Vec<u8> {
        // Get the serialized bytes
        let vec = DataSubscription::vec_to_aligned(subscriptions);
        vec.to_vec()
    }
}

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Debug)]
#[archive(
    compare(PartialEq),
    check_bytes,
)]
#[archive_attr(derive(Debug))]
pub enum DataSubscriptionEvent {
    Subscribed(DataSubscription),
    Unsubscribed(DataSubscription),
    Failed(DataSubscription)
}