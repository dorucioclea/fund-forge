use crate::standardized_types::enums::{MarketType, Resolution};
use crate::standardized_types::subscriptions::{DataSubscription, Symbol};
use chrono::{DateTime, FixedOffset, Utc};
use chrono_tz::Tz;
use crate::apis::data_vendor::datavendor_enum::DataVendor;
use crate::standardized_types::base_data::base_data_enum::BaseDataEnum;

/// Properties are used to update the data que during strategy execution.
pub trait BaseData {
    fn symbol_name(&self) -> Symbol;

    /// The time of the data point in the specified FixedOffset time zone.
    fn time_local(&self, time_zone: &Tz) -> DateTime<FixedOffset>;

    /// UTC time of the data point.
    fn time_utc(&self) -> DateTime<Utc>;
    fn time_created_utc(&self) -> DateTime<Utc>;
    fn time_created_local(&self, time_zone: &Tz) -> DateTime<FixedOffset>;

    fn data_vendor(&self) -> DataVendor;

    fn market_type(&self) -> MarketType;

    fn resolution(&self) -> Resolution;

    fn symbol(&self) -> &Symbol;

    fn subscription(&self) -> DataSubscription;
}
