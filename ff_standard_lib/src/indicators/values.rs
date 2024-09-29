use crate::standardized_types::subscriptions::DataSubscription;
use crate::standardized_types::{Color, Price};
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rkyv::{Archive, Deserialize as Deserialize_rkyv, Serialize as Serialize_rkyv};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use crate::indicators::indicators_trait::IndicatorName;

pub type PlotName = String;

#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct IndicatorPlot {
    pub name: PlotName,
    pub value: Price,
    pub color: Option<Color>,
}

impl IndicatorPlot {
    pub fn new(plot_name: PlotName, value: Price, color: Option<Color>) -> Self {
        Self {
            name: plot_name,
            value,
            color,
        }
    }
}

/// A struct that represents the values of an indicator at a specific time.
#[derive(Clone, Serialize_rkyv, Deserialize_rkyv, Archive, PartialEq, Debug)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct IndicatorValues {
    pub name: IndicatorName,
    pub time: String,
    pub subscription: DataSubscription,
    pub plots: BTreeMap<PlotName, IndicatorPlot>,
}

impl Display for IndicatorValues {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut values_string = String::new();
        for (plot_name, plot) in &self.plots {
            values_string.push_str(&format!("{}: {}\n", plot_name, plot.value));
        }
        write!(f, "{}, {}, {}", self.name, self.subscription, values_string)
    }
}

impl IndicatorValues {
    pub fn new(
        name: IndicatorName,
        subscription: DataSubscription,
        values: BTreeMap<PlotName, IndicatorPlot>,
        time: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            subscription,
            plots: values,
            time: time.to_string(),
        }
    }

    pub fn name(&self) -> &IndicatorName {
        &self.name
    }

    /// get the time in the UTC time zone
    pub fn time_utc(&self) -> DateTime<Utc> {
        DateTime::from_str(&self.time).unwrap()
    }

    /// get the time in the local time zone
    pub fn time_local(&self, time_zone: &Tz) -> DateTime<Tz> {
        time_zone.from_utc_datetime(&self.time_utc().naive_utc())
    }

    /// get the value of a plot by name
    pub fn get_plot(&self, plot_name: &PlotName) -> Option<IndicatorPlot> {
        self.plots.get(plot_name).cloned()
    }

    /// get all the values `values: &AHashMap<PlotName, f64>`
    pub fn plots(&self) -> BTreeMap<PlotName, IndicatorPlot> {
        self.plots.clone()
    }

    /// insert a value into the values
    pub fn insert_plot(&mut self, plot_name: PlotName, value: IndicatorPlot) {
        self.plots.insert(plot_name, value);
    }
}
