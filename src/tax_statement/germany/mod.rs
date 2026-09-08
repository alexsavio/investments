//! German tax statement generation module.
//!
//! Generates comprehensive CSV tax reports for German tax residents with foreign brokerage accounts.

mod csv_formatter;
mod html;
mod processor;
mod report_details;
mod statement;

#[cfg(test)]
mod tests;

use crate::taxes::germany::TeilfreistellungRate;

pub use self::csv_formatter::CsvFormatter;
pub use self::html::{HtmlReport, ReportMeta};
pub use self::processor::{compute_tax_year, process_broker_statement, produces_capital_gain};
pub use self::statement::{
    CapitalGainEntry, DividendEntry, FxGainEntry, GermanTaxStatement, InterestEntry, Section23,
};

// The one-formatter invariant now spans every EUR-filing jurisdiction, not just this one. Re-exported
// so existing `germany::format_eur` call sites keep resolving.
pub(crate) use crate::tax_statement::eur::{format_eur, round_eur};

/// Anlage KAP-INV line numbers for one fund type: (distributions, Vorabpauschale, sale gains/losses).
///
/// The tool classifies funds as equity / mixed / bond; on the form those map to Aktienfonds,
/// Mischfonds, and sonstige Investmentfonds.
///
/// Pinned to the official forms of the Bundesfinanzverwaltung (Formular-Management-System,
/// formulare-bfinv.de): "Anlage KAP-INV 2024" (print id 2024AnlKAP-INV361NET, September 2024) and
/// "Anlage KAP-INV 2025" (2025AnlKAP-INV361NET, September 2025). PDFs retrieved 2026-08-11 from
/// <https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2024/Anlage_KAP_INV_Steuern.de_01.pdf>
/// and <https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2025/Anlage_KAP_INV_2025_steuern-de.pdf>.
/// Both years are identical: "Ausschüttungen nach § 2 Abs. 11 InvStG" on Zeilen 4 / 5 / 8,
/// "Vorabpauschalen nach § 18 InvStG" on Zeilen 9 / 10 / 13, and "Gewinne und Verluste aus der
/// Veräußerung von Investmentanteilen" on Zeilen 14 / 17 / 26 (Aktien- / Misch- / sonstige Fonds).
pub(crate) fn kap_inv_zeilen(rate: TeilfreistellungRate) -> Option<(u32, u32, u32)> {
    match rate {
        TeilfreistellungRate::Equity => Some((4, 9, 14)),
        TeilfreistellungRate::Mixed => Some((5, 10, 17)),
        TeilfreistellungRate::Bond => Some((8, 13, 26)),
        TeilfreistellungRate::None => None,
    }
}
