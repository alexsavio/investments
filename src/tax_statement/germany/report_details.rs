//! The German flavour of the shared report rows: the asset category the German forms distinguish,
//! the foreign-currency treatments §20 and §23 EStG give a movement, and the type aliases that
//! bind [`crate::tax_statement::report::details`] to them.

use crate::tax_statement::report::details;
use crate::taxes::TaxConfig;
use crate::taxes::germany::TeilfreistellungRate;

pub(super) use crate::tax_statement::report::details::{
    BookingKind, LotSource, SaleLotRow, TradeRow, TradeSide, ecb_rate, isin_country,
    security_identity,
};

pub type ReportDetails = details::ReportDetails<AssetCategory, FxTreatment>;
pub type SaleWorksheet = details::SaleWorksheet<AssetCategory>;
pub type OpenLotRow = details::OpenLotRow<AssetCategory>;
pub type BookingRow = details::BookingRow<AssetCategory>;
pub type WithholdingRow = details::WithholdingRow<AssetCategory>;
pub type FxRow = details::FxRow<FxTreatment>;

/// Asset class as the German forms distinguish it, derived from the Teilfreistellung rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetCategory {
    Stock,
    EquityFund,
    MixedFund,
    OtherFund,
}

impl From<TeilfreistellungRate> for AssetCategory {
    fn from(rate: TeilfreistellungRate) -> Self {
        match rate {
            TeilfreistellungRate::None => AssetCategory::Stock,
            TeilfreistellungRate::Equity => AssetCategory::EquityFund,
            TeilfreistellungRate::Mixed => AssetCategory::MixedFund,
            TeilfreistellungRate::Bond => AssetCategory::OtherFund,
        }
    }
}

impl From<AssetCategory> for TeilfreistellungRate {
    fn from(category: AssetCategory) -> Self {
        match category {
            AssetCategory::Stock => TeilfreistellungRate::None,
            AssetCategory::EquityFund => TeilfreistellungRate::Equity,
            AssetCategory::MixedFund => TeilfreistellungRate::Mixed,
            AssetCategory::OtherFund => TeilfreistellungRate::Bond,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxTreatment {
    /// §20 EStG capital income (interest-bearing account, held currency disposed).
    Section20Taxable,
    /// Repayment of a foreign-currency loan: not taxable (BMF 19.05.2022 Rz. 131).
    LoanRepaymentNonTaxable,
    /// §23 EStG private sale within one year (taxable, Anlage SO).
    Section23ShortTerm,
    /// §23 EStG private sale after more than one year (tax-free).
    Section23LongTermTaxFree,
    /// Repayment of borrowed currency under §23: needs manual review.
    Section23BorrowedReview,
}

/// Teilfreistellung classification by ISIN, exactly as the tax entries derive it.
pub(super) fn classify(tax_config: &TaxConfig, isin: &str) -> TeilfreistellungRate {
    if isin.is_empty() {
        TeilfreistellungRate::None
    } else {
        tax_config
            .get_etf_classification(isin)
            .to_teilfreistellung_rate()
    }
}
