use serde::Deserialize;

use crate::time::{deserialize_date, deserialize_optional_date};
use crate::types::{Date, Decimal};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Operation {
    Buy(BuyOperation),
    Sell(SellOperation),
    Dividend(DividendOperation),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuyOperation {
    #[serde(deserialize_with = "deserialize_date")]
    pub date: Date,
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub settle_date: Option<Date>,

    pub symbol: String,
    pub quantity: Decimal,

    pub price: Decimal,
    pub amount: Decimal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SellOperation {
    #[serde(deserialize_with = "deserialize_date")]
    pub date: Date,
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub settle_date: Option<Date>,

    pub symbol: String,
    pub quantity: Decimal,

    pub price: Decimal,
    pub net_amount: Decimal,
    #[serde(default)]
    pub tax_withheld: Decimal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DividendOperation {
    #[serde(deserialize_with = "deserialize_date")]
    pub date: Date,
    pub symbol: String,
    pub gross_amount: Decimal,
    pub tax_withheld: Decimal,
    pub net_amount: Decimal,
}
