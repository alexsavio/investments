//! The Spanish flavour of the shared report rows: the asset class the return distinguishes, the
//! two foreign-currency treatments, and the type aliases that bind
//! [`crate::tax_statement::report::details`] to them.

use crate::instruments::EtfClassification;
use crate::tax_statement::report::details;
use crate::taxes::TaxConfig;

pub(super) use crate::tax_statement::report::details::{
    BookingKind, LotSource, SaleLotRow, TradeRow, TradeSide, ecb_rate, isin_country,
    security_identity,
};

pub type ReportDetails = details::ReportDetails<AssetClass, FxTreatment>;
pub type SaleWorksheet = details::SaleWorksheet<AssetClass>;
pub type OpenLotRow = details::OpenLotRow<AssetClass>;
pub type BookingRow = details::BookingRow<AssetClass>;
pub type WithholdingRow = details::WithholdingRow<AssetClass>;
pub type FxRow = details::FxRow<FxTreatment>;

/// What the return makes of an instrument.
///
/// The savings base treats a share and a fund holding alike, so this is a reading aid rather than a
/// tax decision — except where it is not: the Gipuzkoa €1,500 dividend exemption does not reach
/// distributions from instituciones de inversión colectiva, and the report names the payers it
/// classified as funds so the filer can check them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetClass {
    Stock,
    Fund,
}

/// Which balance a foreign-currency result was realized on.
///
/// Held balances are transfers of a patrimonial element and enter the ganancias group; borrowed
/// ones are referred for manual review, because repaying a currency loan is not clearly such a
/// transfer and none of NF 3/2014, the LIRPF and the TRLFIRPF settles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxTreatment {
    HeldBalance,
    BorrowedBalance,
}

/// Asset class by ISIN, from the same `taxes.etf_classification` map the German statement reads.
///
/// An unclassified instrument is a share, which is exactly what the processor already assumes: no
/// Spanish figure turns on the classification, so an unconfigured ISIN cannot change a tax result.
pub(super) fn classify(tax_config: &TaxConfig, isin: &str) -> AssetClass {
    if !isin.is_empty() && tax_config.get_etf_classification(isin) != EtfClassification::None {
        AssetClass::Fund
    } else {
        AssetClass::Stock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_configured_isin_is_a_fund() {
        let mut config = TaxConfig::default();
        config
            .etf_classification
            .insert("IE00B4L5Y983".to_owned(), EtfClassification::Equity);

        assert_eq!(classify(&config, "IE00B4L5Y983"), AssetClass::Fund);
        assert_eq!(classify(&config, "US0378331005"), AssetClass::Stock);
        // An instrument the statement carries no ISIN for cannot be looked up at all.
        assert_eq!(classify(&config, ""), AssetClass::Stock);
    }
}
