//! Broker-level detail recorded alongside the tax entries for the printable HTML report.
//!
//! The processor fills these rows inside the same loops that compute the tax entries, from the
//! same intermediate values, so the report reproduces the CSV/console EUR figures exactly. The
//! renderer only reads this data and never recomputes a tax figure. Rows are self-contained
//! (symbol, ISIN, name, dates, amounts), so the renderer never joins across vectors.

use crate::broker_statement::BrokerStatement;
use crate::core::GenericResult;
use crate::currency::converter::CurrencyConverter;
use crate::taxes::TaxConfig;
use crate::taxes::germany::TeilfreistellungRate;
use crate::time::Date;
use crate::types::Decimal;

/// Everything the report shows beyond the tax entries themselves.
#[derive(Debug, Default)]
pub struct ReportDetails {
    /// Every security that was traded, held or paid income in the year (Wertpapierübersicht).
    pub securities: Vec<SecurityRow>,
    /// Raw buys and sells of the year in original currency (Wertpapiertransaktionen).
    pub trades: Vec<TradeRow>,
    /// One FIFO worksheet per capital-gain entry, in the same order as `capital_gains`.
    pub sales: Vec<SaleWorksheet>,
    /// Purchase lots still (partly) unsold at `open_lots_as_of` (Offene Positionen).
    pub open_lots: Vec<OpenLotRow>,
    /// Cash bookings: dividends, withholding, interest, fees, cash grants (Barwirksame Buchungen).
    pub bookings: Vec<BookingRow>,
    /// Foreign withholding per dividend (Quellensteuer-Übersicht).
    pub withholding: Vec<WithholdingRow>,
    /// Every foreign-currency movement portion of the year (Fremdwährungsgewinne/-verluste).
    pub fx_rows: Vec<FxRow>,
    /// Date the open lots are valid for: the tax year end, or the statement end when earlier.
    pub open_lots_as_of: Option<Date>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LotSource {
    Trade,
    Grant,
    CorporateAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BookingKind {
    Dividend,
    WithholdingTax,
    Interest,
    Fee,
    CashGrant,
}

/// How a foreign-currency result is treated for tax.
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

#[derive(Debug, Clone)]
pub struct SecurityRow {
    pub symbol: String,
    pub isin: String,
    pub name: String,
    /// Two-letter country code from the ISIN prefix; empty when the ISIN is unknown.
    pub country_code: String,
    /// Trading currency (from the first trade or booking); empty when unknown.
    pub currency: String,
    pub category: AssetCategory,
    pub teilfreistellung_rate: TeilfreistellungRate,
}

/// A raw trade in original currency. Sign convention: cash impact (buys negative, sells positive).
#[derive(Debug, Clone)]
pub struct TradeRow {
    pub date: Date,
    pub settle_date: Date,
    pub trade_id: Option<String>,
    pub symbol: String,
    pub isin: String,
    pub name: String,
    pub side: TradeSide,
    pub quantity: Decimal,
    pub currency: String,
    pub price: Decimal,
    /// Trade volume with cash sign (negative for buys).
    pub gross: Decimal,
    /// Commission as a cash outflow (negative or zero).
    pub commission: Decimal,
    /// `gross + commission`.
    pub net: Decimal,
    /// ECB reference rate on the settlement date: EUR per one unit of `currency`.
    pub eur_per_unit: Decimal,
    /// `net` in EUR, converted like the FIFO engine does (volume at settlement, commission at
    /// conclusion). For sells this is exactly the entry's `proceeds_eur`.
    pub amount_eur: Decimal,
}

/// One purchase lot consumed by a sale.
#[derive(Debug, Clone)]
pub struct SaleLotRow {
    pub open_date: Date,
    pub open_trade_id: Option<String>,
    pub source: LotSource,
    /// Quantity taken from this lot, in the sale's share units (split-adjusted).
    pub quantity: Decimal,
    /// Purchase price per share in original currency, when the lot came from a trade.
    pub price: Option<Decimal>,
    /// EUR cost of this lot portion incl. purchase commission: exactly the addend of the entry's
    /// `cost_basis_eur` (vest-date FMV for grant lots).
    pub cost_eur: Decimal,
    /// Pro-rata share of the sale's net proceeds (full precision; informational).
    pub proceeds_eur: Decimal,
    /// `proceeds_eur - cost_eur` (informational; the entry's gain is authoritative).
    pub gain_loss_eur: Decimal,
    pub holding_days: i64,
    /// Lot acquired before 2009 (Altbestand, grandfathered).
    pub pre_2009: bool,
}

/// FIFO worksheet of one sale (one per `CapitalGainEntry`).
#[derive(Debug, Clone)]
pub struct SaleWorksheet {
    pub symbol: String,
    pub isin: String,
    pub name: String,
    pub category: AssetCategory,
    pub sale_date: Date,
    pub settle_date: Date,
    pub trade_id: Option<String>,
    pub quantity: Decimal,
    pub currency: String,
    pub price: Decimal,
    /// Sale volume in original currency (positive).
    pub gross: Decimal,
    /// Sale commission in original currency (positive).
    pub commission: Decimal,
    /// ECB reference rate on the settlement date: EUR per one unit of `currency`.
    pub eur_per_unit: Decimal,
    /// Net proceeds in EUR (the entry's `proceeds_eur`).
    pub proceeds_eur: Decimal,
    /// The entry's `cost_basis_eur`.
    pub cost_basis_eur: Decimal,
    /// The entry's `gross_gain_loss`.
    pub gain_loss_eur: Decimal,
    pub notes: Option<String>,
    pub lots: Vec<SaleLotRow>,
}

/// A purchase lot still (partly) open at the report's as-of date.
#[derive(Debug, Clone)]
pub struct OpenLotRow {
    pub symbol: String,
    pub isin: String,
    pub name: String,
    pub category: AssetCategory,
    pub open_date: Date,
    pub trade_id: Option<String>,
    pub source: LotSource,
    /// Unsold quantity.
    pub quantity: Decimal,
    /// Original currency; empty for grant / corporate-action lots.
    pub currency: String,
    pub price: Option<Decimal>,
    /// EUR cost of the unsold part, incl. purchase commission (vest-date FMV for grant lots).
    pub cost_eur: Decimal,
}

/// A cash booking that fed a tax entry.
#[derive(Debug, Clone)]
pub struct BookingRow {
    pub kind: BookingKind,
    pub date: Date,
    /// Empty for bookings not tied to a security (interest, fees, cash grants).
    pub symbol: String,
    pub isin: String,
    pub name: String,
    pub category: Option<AssetCategory>,
    pub description: String,
    pub currency: String,
    /// Amount in original currency with cash sign (withholding and fees negative).
    pub amount: Decimal,
    /// ECB reference rate on the booking date: EUR per one unit of `currency`.
    pub eur_per_unit: Decimal,
    /// `amount` in EUR: the exact value the tax entry uses (with cash sign).
    pub amount_eur: Decimal,
}

/// Foreign withholding tax on one dividend.
#[derive(Debug, Clone)]
pub struct WithholdingRow {
    pub date: Date,
    pub symbol: String,
    pub isin: String,
    pub name: String,
    pub country_code: String,
    pub category: AssetCategory,
    pub currency: String,
    pub gross: Decimal,
    pub gross_eur: Decimal,
    /// Withheld amount in original currency (positive).
    pub withheld: Decimal,
    /// Withheld amount in EUR (positive; the entry's `foreign_withholding_tax`).
    pub withheld_eur: Decimal,
    /// `withheld / gross` as a fraction (e.g. 0.15).
    pub withholding_rate: Decimal,
    /// The entry's `foreign_tax_credit` (zero for fund distributions under InvStG).
    pub creditable_eur: Decimal,
}

/// One portion of a foreign-currency movement, in ledger order.
#[derive(Debug, Clone)]
pub struct FxRow {
    pub currency: String,
    pub date: Date,
    pub transaction_id: String,
    pub activity_code: String,
    /// Signed units of this portion (positive = inflow).
    pub units: Decimal,
    /// EUR per unit of the movement.
    pub eur_per_unit: Decimal,
    /// `units × eur_per_unit`.
    pub amount_eur: Decimal,
    /// Acquisition details of the consumed lot; `None` for an acquisition row.
    pub open_date: Option<Date>,
    pub open_eur_per_unit: Option<Decimal>,
    /// `|units| × open_eur_per_unit`.
    pub open_value_eur: Option<Decimal>,
    /// Realized result in EUR. For §20 taxable rows this is the cent-rounded value the tax entry
    /// uses; otherwise full precision (the section totals round once).
    pub gain_loss_eur: Option<Decimal>,
    pub balance_after: Decimal,
    pub holding_days: Option<i64>,
    pub treatment: Option<FxTreatment>,
}

/// ISIN (empty when unknown) and plain display name of a security: the configured name, else the
/// broker's own description, else the symbol.
pub(super) fn security_identity(
    broker_statement: &BrokerStatement,
    symbol: &str,
) -> (String, String) {
    let instrument = broker_statement.instrument_info.get(symbol);
    let isin = instrument
        .and_then(|info| info.isin.iter().next())
        .map(|isin| isin.to_string())
        .unwrap_or_default();
    let name = instrument
        .and_then(|info| info.name().or_else(|| info.description()))
        .map(str::to_owned)
        .unwrap_or_else(|| symbol.to_owned());
    (isin, name)
}

/// Two-letter country code from an ISIN prefix; empty when there is no (valid) ISIN.
pub(super) fn isin_country(isin: &str) -> String {
    isin.get(..2)
        .filter(|prefix| prefix.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_uppercase)
        .unwrap_or_default()
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

/// ECB reference rate for display: EUR per one unit of `currency` on `date`.
pub(super) fn ecb_rate(
    converter: &CurrencyConverter,
    date: Date,
    currency: &str,
) -> GenericResult<Decimal> {
    if currency == "EUR" {
        return Ok(dec!(1));
    }
    converter.currency_rate(date, currency, "EUR").map_err(|e| {
        format!(
            "Failed to look up the ECB rate for {currency} on {date}. This may indicate missing \
             ECB exchange rates. Ensure your database has currency rates for this date. Error: {e}"
        )
        .into()
    })
}
