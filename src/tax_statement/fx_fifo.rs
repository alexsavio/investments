//! Per-currency signed-inventory FIFO for realized foreign-currency results.
//!
//! Foreign-currency cash is replayed in statement order, and the engine reports what each movement
//! realized. Only the split it draws is jurisdiction-neutral: a **held** balance (positive running
//! quantity) and a **borrowed** one (negative). Which of the two is taxable, and under which head,
//! is the caller's decision — Germany routes held-balance results to §20 and treats borrowed-balance
//! repayments as non-taxable (BMF 19.05.2022 Rz. 131), while Spain integrates held-balance results
//! into the savings base and refers borrowed-balance results for manual review.
//!
//! Each disposal of held currency (a securities purchase, a fee, a reconversion to EUR) realizes a
//! result against the held lots; each repayment of borrowed currency realizes one against the
//! borrowed lots. Real currency exchanges are valued at the actual execution rate (the paired EUR
//! leg); every other movement — including securities bought directly in the foreign currency — is
//! valued at the central-bank reference rate of the transaction date.

use std::collections::{HashMap, VecDeque};

use crate::broker_statement::ForeignCashFlow;
use crate::core::GenericResult;
use crate::time::Date;
use crate::types::Decimal;

/// Realized FX result for one foreign currency over the statement.
#[derive(Debug)]
pub struct CurrencyFxResult {
    pub currency: String,
    /// Results realized against a held (positive) balance.
    pub taxable: Vec<FxRealization>,
    /// Results realized against a borrowed (negative) balance, dated so the caller can year-filter
    /// them exactly as it does the held-balance ones.
    pub non_taxable: Vec<FxRealization>,
    /// Every movement portion in ledger order (report worksheet; not used by the tax path).
    pub ledger: Vec<FxLedgerRow>,
}

/// A single realized FX gain/loss in EUR (full precision; positive = gain, negative = loss).
#[derive(Debug)]
pub struct FxRealization {
    /// Disposal date (the movement that realized the gain/loss).
    pub date: Date,
    /// Acquisition date of the consumed lot, for callers whose rules depend on a holding period.
    pub acquisition_date: Date,
    pub amount: Decimal,
    pub activity_code: String,
}

/// One FIFO portion of a foreign-currency movement, in ledger order. Every movement appears here —
/// acquisitions as well as disposals — so a report can show the full currency worksheet; the tax
/// path reads only `CurrencyFxResult::taxable` / `non_taxable`.
#[derive(Debug, Clone)]
pub struct FxLedgerRow {
    pub date: Date,
    /// Broker transaction id of the movement (empty when the statement has none).
    pub transaction_id: String,
    pub activity_code: String,
    /// Signed units of this portion: positive = inflow, negative = outflow.
    pub units: Decimal,
    /// EUR value of one unit for this movement (execution rate for a real exchange, else ECB).
    pub rate: Decimal,
    pub kind: FxLedgerKind,
    /// Currency balance after this portion.
    pub balance_after: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxLedgerKind {
    /// Opened or extended a lot (held when `units > 0`, borrowed when `units < 0`); no result.
    Acquisition,
    /// Disposed held currency: a §20 gain/loss `amount` (EUR, full precision) against a lot opened
    /// on `acquisition_date` at `acquisition_rate`.
    Disposal {
        acquisition_date: Date,
        acquisition_rate: Decimal,
        amount: Decimal,
    },
    /// Repaid borrowed currency: the non-taxable result `amount` against the borrowing lot.
    Repayment {
        acquisition_date: Date,
        acquisition_rate: Decimal,
        amount: Decimal,
    },
}

/// A pre-history foreign-currency lot declared in config, seeding the FIFO for a statement that does
/// not begin from a zero balance (truncated history). The FIFO opens the currency from this lot
/// instead of empty; the statement's running balance must still reconcile from the declared opening
/// onward, so a wrong quantity is still caught by the balance-chain check. The declared rate is
/// trusted — it is the acquisition rate the truncated statement cannot supply.
pub struct OpeningLot {
    pub currency: String,
    /// Signed quantity at the statement's start: positive = held (Guthaben), negative = borrowed.
    pub quantity: Decimal,
    /// EUR value of one unit of the foreign currency at acquisition.
    pub eur_per_unit: Decimal,
    /// Acquisition date of the opening lot.
    pub date: Date,
}

/// One FIFO lot: a signed quantity of the foreign currency carrying its EUR-per-unit rate.
struct Lot {
    /// Remaining quantity: positive = held (Guthaben), negative = borrowed (Kredit).
    qty: Decimal,
    /// EUR value of one unit of the foreign currency at acquisition.
    rate: Decimal,
    /// Acquisition date of this lot.
    date: Date,
}

/// Replay the cash-flow ledger through a per-currency signed-inventory FIFO.
///
/// `ecb_rate(date, currency)` returns the central-bank reference rate as EUR per unit of `currency`.
/// EUR movements are the filing currency (no FX gain against themselves) and are skipped. Results
/// are returned in first-seen currency order.
///
/// `opening` seeds each currency's inventory before the flows are replayed, for a statement whose
/// history does not start from a zero balance. Each opening lot moves that currency's starting
/// balance from zero to the declared quantity; the balance-chain check then validates the flows
/// against the declared opening, so a wrong quantity is still rejected. Pass an empty slice for the
/// common full-history case.
pub fn compute_fx_fifo<R>(
    flows: &[ForeignCashFlow],
    opening: &[OpeningLot],
    ecb_rate: R,
) -> GenericResult<Vec<CurrencyFxResult>>
where
    R: Fn(Date, &str) -> GenericResult<Decimal>,
{
    let mut order: Vec<String> = Vec::new();
    let mut lots: HashMap<String, VecDeque<Lot>> = HashMap::new();
    let mut results: HashMap<String, CurrencyFxResult> = HashMap::new();
    let mut running: HashMap<String, Decimal> = HashMap::new();

    for lot in opening {
        if lot.currency == "EUR" || lot.quantity == dec!(0) {
            continue;
        }
        register_currency(&mut order, &mut lots, &mut results, &lot.currency);
        *running.entry(lot.currency.clone()).or_insert(dec!(0)) += lot.quantity;
        lots.get_mut(&lot.currency).unwrap().push_back(Lot {
            qty: lot.quantity,
            rate: lot.eur_per_unit,
            date: lot.date,
        });
        results
            .get_mut(&lot.currency)
            .unwrap()
            .ledger
            .push(FxLedgerRow {
                date: lot.date,
                transaction_id: String::new(),
                activity_code: "OPENING".to_string(),
                units: lot.quantity,
                rate: lot.eur_per_unit,
                kind: FxLedgerKind::Acquisition,
                balance_after: lot.quantity,
            });
    }

    for flow in flows {
        if flow.currency == "EUR" || flow.amount == dec!(0) {
            continue;
        }

        // The FIFO can only value what it replays, so it opens each currency from a known balance:
        // zero, or the declared `opening` lot. Verify that against the statement's own running
        // balance: the movements must sum, in document order from that opening, to the reported
        // balance after each step. A mismatch means an undeclared carried-in balance (foreign cash
        // held across the year boundary, whose prior-year acquisition rate this statement does not
        // carry), out-of-order rows, or dropped rows — each of which would silently mis-classify §20
        // gains, so refuse rather than guess a tax figure.
        let running_balance = running.entry(flow.currency.clone()).or_insert(dec!(0));
        *running_balance += flow.amount;
        if (*running_balance - flow.balance).abs() > dec!(0.01) {
            return Err(format!(
                "Foreign-currency ledger for {} is inconsistent on {}: movements sum to {} but the \
                 statement balance is {}. The Statement of Funds must reconcile from the opening \
                 balance in document order. If the export does not begin at account opening, declare \
                 the carried-in {} balance via the portfolio's opening_foreign_currency config; \
                 out-of-order or dropped rows are not supported.",
                flow.currency, flow.date, running_balance, flow.balance, flow.currency
            )
            .into());
        }

        // EUR value per unit of the currency for this movement: the actual execution rate for a
        // real exchange (the paired EUR leg, which carries the opposite sign), otherwise the ECB
        // reference rate.
        let rate = match (flow.activity_code.as_str(), flow.eur_execution) {
            ("FOREX", Some(eur_execution)) => {
                // A real conversion's two legs have opposite signs, so -eur/amount is positive. A
                // same-sign or zero EUR leg (a malformed export or a mis-paired transactionID)
                // would yield a non-positive rate and fabricate a large gain; refuse instead.
                let rate = -eur_execution / flow.amount;
                if rate <= dec!(0) {
                    return Err(format!(
                        "Malformed FOREX leg for {} on {}: EUR execution {} against movement {} \
                         gives a non-positive rate {}. A real conversion's legs carry opposite \
                         signs; refusing rather than booking a fabricated gain.",
                        flow.currency, flow.date, eur_execution, flow.amount, rate
                    )
                    .into());
                }
                rate
            }
            _ => ecb_rate(flow.date, &flow.currency)?,
        };

        register_currency(&mut order, &mut lots, &mut results, &flow.currency);
        let lots = lots.get_mut(&flow.currency).unwrap();
        let result = results.get_mut(&flow.currency).unwrap();

        let mut remaining = flow.amount;
        while remaining != dec!(0) {
            let extend = lots
                .front()
                .is_none_or(|front| front.qty.is_sign_positive() == remaining.is_sign_positive());
            if extend {
                lots.push_back(Lot {
                    qty: remaining,
                    rate,
                    date: flow.date,
                });
                result.ledger.push(FxLedgerRow {
                    date: flow.date,
                    transaction_id: flow.transaction_id.clone(),
                    activity_code: flow.activity_code.clone(),
                    units: remaining,
                    rate,
                    kind: FxLedgerKind::Acquisition,
                    balance_after: flow.balance,
                });
                remaining = dec!(0);
                continue;
            }

            let front = lots.front_mut().unwrap();
            let units = front.qty.abs().min(remaining.abs());
            let kind = if front.qty.is_sign_positive() {
                // Disposing held currency → §20 taxable gain/loss.
                let amount = units * (rate - front.rate);
                result.taxable.push(FxRealization {
                    date: flow.date,
                    acquisition_date: front.date,
                    amount,
                    activity_code: flow.activity_code.clone(),
                });
                FxLedgerKind::Disposal {
                    acquisition_date: front.date,
                    acquisition_rate: front.rate,
                    amount,
                }
            } else {
                // Repaying borrowed currency → non-taxable (Tilgung Fremdwährungskredit).
                let amount = units * (front.rate - rate);
                result.non_taxable.push(FxRealization {
                    date: flow.date,
                    acquisition_date: front.date,
                    amount,
                    activity_code: flow.activity_code.clone(),
                });
                FxLedgerKind::Repayment {
                    acquisition_date: front.date,
                    acquisition_rate: front.rate,
                    amount,
                }
            };
            front.qty -= sign(front.qty) * units;
            remaining -= sign(remaining) * units;
            // The statement balance is reported per movement; this portion's balance is the
            // statement balance minus whatever of the movement is still unprocessed.
            result.ledger.push(FxLedgerRow {
                date: flow.date,
                transaction_id: flow.transaction_id.clone(),
                activity_code: flow.activity_code.clone(),
                units: sign(flow.amount) * units,
                rate,
                kind,
                balance_after: flow.balance - remaining,
            });
            if front.qty == dec!(0) {
                lots.pop_front();
            }
        }
    }

    Ok(order
        .into_iter()
        .map(|currency| results.remove(&currency).unwrap())
        .collect())
}

/// Register a currency's inventory, result bucket, and first-seen order slot on first use.
fn register_currency(
    order: &mut Vec<String>,
    lots: &mut HashMap<String, VecDeque<Lot>>,
    results: &mut HashMap<String, CurrencyFxResult>,
    currency: &str,
) {
    if results.contains_key(currency) {
        return;
    }
    order.push(currency.to_string());
    lots.insert(currency.to_string(), VecDeque::new());
    results.insert(
        currency.to_string(),
        CurrencyFxResult {
            currency: currency.to_string(),
            taxable: Vec::new(),
            non_taxable: Vec::new(),
            ledger: Vec::new(),
        },
    );
}

/// Sign of a non-zero decimal as `+1` / `-1`.
fn sign(value: Decimal) -> Decimal {
    if value.is_sign_positive() {
        dec!(1)
    } else {
        dec!(-1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::collections::HashMap;

    fn flow(
        day: u32,
        code: &str,
        amount: Decimal,
        balance: Decimal,
        eur_execution: Option<Decimal>,
    ) -> ForeignCashFlow {
        ForeignCashFlow {
            currency: "USD".to_string(),
            date: Date::from_ymd_opt(2025, 11, day).unwrap(),
            transaction_id: String::new(),
            activity_code: code.to_string(),
            amount,
            balance,
            eur_execution,
        }
    }

    /// ECB rate lookup backed by a per-day table (EUR per USD).
    fn rates(entries: &[(u32, Decimal)]) -> impl Fn(Date, &str) -> GenericResult<Decimal> + '_ {
        let table: HashMap<u32, Decimal> = entries.iter().copied().collect();
        move |date: Date, _ccy: &str| {
            table
                .get(&(date.day()))
                .copied()
                .ok_or_else(|| format!("no rate for {date}").into())
        }
    }

    /// A securities purchase that settles before the covering EUR→USD forex drives the USD balance
    /// negative: the whole sequence is a Fremdwährungskredit repaid at a loss → non-taxable, no §20.
    #[test]
    fn buy_first_loan_is_non_taxable() {
        let flows = vec![
            // Buy debits USD at the ECB rate 0.90 → balance -1000 (borrowed).
            flow(4, "BUY", dec!(-1000), dec!(-1000), None),
            // Forex brings 1000 USD in, paying 910 EUR (execution 0.91) → repays the loan.
            flow(4, "FOREX", dec!(1000), dec!(0), Some(dec!(-910))),
        ];
        let results = compute_fx_fifo(&flows, &[], rates(&[(4, dec!(0.90))])).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].taxable.is_empty());
        // Repaid 1000 USD borrowed at 0.90 with USD costing 0.91 → 1000*(0.90-0.91) = -10.
        assert_eq!(results[0].non_taxable.len(), 1);
        assert_eq!(results[0].non_taxable[0].amount, dec!(-10));

        // The ledger shows both portions: the borrowing (acquisition of a negative lot) and the
        // repayment against it.
        let ledger = &results[0].ledger;
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger[0].units, dec!(-1000));
        assert_eq!(ledger[0].rate, dec!(0.90));
        assert_eq!(ledger[0].kind, FxLedgerKind::Acquisition);
        assert_eq!(ledger[0].balance_after, dec!(-1000));
        assert_eq!(ledger[1].units, dec!(1000));
        assert_eq!(ledger[1].rate, dec!(0.91));
        assert_eq!(
            ledger[1].kind,
            FxLedgerKind::Repayment {
                acquisition_date: Date::from_ymd_opt(2025, 11, 4).unwrap(),
                acquisition_rate: dec!(0.90),
                amount: dec!(-10),
            }
        );
        assert_eq!(ledger[1].balance_after, dec!(0));
    }

    /// A forex conversion that runs before the purchase leaves a positive USD balance; the later
    /// purchase disposes that Guthaben at a higher ECB rate → a taxable §20 "verdeckter" gain.
    #[test]
    fn forex_first_guthaben_disposal_is_taxable() {
        let flows = vec![
            // Forex acquires 1000 USD at execution 0.8588 → balance +1000 (held).
            flow(13, "FOREX", dec!(1000), dec!(1000), Some(dec!(-858.80))),
            // Buy disposes 1000 USD valued at the ECB rate 0.8607.
            flow(13, "BUY", dec!(-1000), dec!(0), None),
        ];
        let results = compute_fx_fifo(&flows, &[], rates(&[(13, dec!(0.8607))])).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].non_taxable.is_empty());
        assert_eq!(results[0].taxable.len(), 1);
        // 1000*(0.8607-0.8588) = 1.90.
        assert_eq!(results[0].taxable[0].amount, dec!(1.90));

        let ledger = &results[0].ledger;
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger[0].kind, FxLedgerKind::Acquisition);
        assert_eq!(ledger[0].units, dec!(1000));
        assert_eq!(ledger[0].rate, dec!(0.8588));
        assert_eq!(ledger[1].units, dec!(-1000));
        assert_eq!(ledger[1].rate, dec!(0.8607));
        assert_eq!(
            ledger[1].kind,
            FxLedgerKind::Disposal {
                acquisition_date: Date::from_ymd_opt(2025, 11, 13).unwrap(),
                acquisition_rate: dec!(0.8588),
                amount: dec!(1.90),
            }
        );
        assert_eq!(ledger[1].balance_after, dec!(0));
    }

    /// A dividend received in USD and held across days, then converted back to EUR at a lower rate,
    /// realizes a taxable §20 loss on the Guthaben.
    #[test]
    fn cross_day_dividend_reconversion_is_taxable() {
        let flows = vec![
            // Dividend of 100 USD at ECB 0.86 → held.
            flow(1, "DIV", dec!(100), dec!(100), None),
            // Reconvert 100 USD to EUR, receiving 85 EUR (execution 0.85).
            flow(2, "FOREX", dec!(-100), dec!(0), Some(dec!(85))),
        ];
        let results = compute_fx_fifo(&flows, &[], rates(&[(1, dec!(0.86))])).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].non_taxable.is_empty());
        assert_eq!(results[0].taxable.len(), 1);
        // 100*(0.85-0.86) = -1.00.
        assert_eq!(results[0].taxable[0].amount, dec!(-1.00));
    }

    /// A ledger whose first row already carries a nonzero balance (a position carried in from a
    /// prior year) breaks the running-balance invariant and is refused rather than mis-classified.
    #[test]
    fn nonzero_opening_balance_is_rejected() {
        let flows = vec![
            // Buy of 1000 USD but the balance shows -6000: 5000 USD was already borrowed on 1 Jan.
            flow(4, "BUY", dec!(-1000), dec!(-6000), None),
        ];
        let err = compute_fx_fifo(&flows, &[], rates(&[(4, dec!(0.90))])).unwrap_err();
        assert!(err.to_string().contains("inconsistent"), "{err}");
    }

    /// A declared opening lot seeds the inventory so a truncated-history statement reconciles: the
    /// disposal is valued against the declared acquisition rate and dated, and the balance chain now
    /// validates from the declared opening.
    #[test]
    fn declared_opening_lot_is_accepted() {
        // 1000 USD held at year start, acquired at 0.90 on 2023-06-01 (before this statement).
        let opening = vec![OpeningLot {
            currency: "USD".to_string(),
            quantity: dec!(1000),
            eur_per_unit: dec!(0.90),
            date: Date::from_ymd_opt(2023, 6, 1).unwrap(),
        }];
        // Spend 400 USD (buy) at ECB 0.95; balance falls from the seeded 1000 to 600.
        let flows = vec![flow(4, "BUY", dec!(-400), dec!(600), None)];
        let results = compute_fx_fifo(&flows, &opening, rates(&[(4, dec!(0.95))])).unwrap();

        assert_eq!(results.len(), 1);
        let taxable = &results[0].taxable;
        assert_eq!(taxable.len(), 1);
        assert_eq!(taxable[0].amount, dec!(20.00)); // 400 * (0.95 - 0.90)
        assert_eq!(
            taxable[0].acquisition_date,
            Date::from_ymd_opt(2023, 6, 1).unwrap()
        );
    }

    /// A declared opening lot whose quantity does not reconcile with the statement's own running
    /// balance is still refused: the escape hatch trusts the rate, not a wrong balance.
    #[test]
    fn wrong_declared_opening_quantity_is_still_rejected() {
        let opening = vec![OpeningLot {
            currency: "USD".to_string(),
            quantity: dec!(1000), // claims +1000 held...
            eur_per_unit: dec!(0.90),
            date: Date::from_ymd_opt(2023, 6, 1).unwrap(),
        }];
        // ...but the first row's balance -6000 after a -1000 buy implies the opening was -5000.
        let flows = vec![flow(4, "BUY", dec!(-1000), dec!(-6000), None)];
        let err = compute_fx_fifo(&flows, &opening, rates(&[(4, dec!(0.90))])).unwrap_err();
        assert!(err.to_string().contains("inconsistent"), "{err}");
    }

    /// Rows presented out of document order break the balance chain and are refused, because order
    /// alone decides whether a movement is a taxable Guthaben disposal or a non-taxable repayment.
    #[test]
    fn out_of_order_rows_are_rejected() {
        let flows = vec![
            // True order is FOREX (+1000, balance 1000) then BUY (-1000, balance 0); swapping the
            // rows leaves each carrying the other's balance.
            flow(13, "BUY", dec!(-1000), dec!(0), None),
            flow(13, "FOREX", dec!(1000), dec!(1000), Some(dec!(-858.80))),
        ];
        let err = compute_fx_fifo(&flows, &[], rates(&[(13, dec!(0.8607))])).unwrap_err();
        assert!(err.to_string().contains("inconsistent"), "{err}");
    }

    /// A lot acquired in one year and disposed across later years keeps its original acquisition
    /// rate: multi-year statements (merged in period order) value a carried-over position correctly,
    /// and each disposal is dated to its own year for the caller's year filter.
    #[test]
    fn carries_lot_rate_across_year_boundaries() {
        let mk =
            |year: i32, code: &str, amount: Decimal, balance: Decimal, eur: Option<Decimal>| {
                ForeignCashFlow {
                    currency: "USD".to_string(),
                    date: Date::from_ymd_opt(year, 11, 4).unwrap(),
                    transaction_id: String::new(),
                    activity_code: code.to_string(),
                    amount,
                    balance,
                    eur_execution: eur,
                }
            };
        let flows = vec![
            // Acquire 1000 USD at execution 0.90 in 2023.
            mk(2023, "FOREX", dec!(1000), dec!(1000), Some(dec!(-900))),
            // Dispose 500 USD at ECB 0.95 in 2024, then 500 at ECB 1.00 in 2025.
            mk(2024, "BUY", dec!(-500), dec!(500), None),
            mk(2025, "BUY", dec!(-500), dec!(0), None),
        ];
        let ecb = |date: Date, _ccy: &str| -> GenericResult<Decimal> {
            Ok(match date.year() {
                2024 => dec!(0.95),
                2025 => dec!(1.00),
                other => return Err(format!("no rate for {other}").into()),
            })
        };
        let results = compute_fx_fifo(&flows, &[], ecb).unwrap();
        assert_eq!(results.len(), 1);
        let taxable = &results[0].taxable;
        assert_eq!(taxable.len(), 2);
        // Both disposals are valued against the 2023 acquisition rate 0.90, not against each other.
        assert_eq!(taxable[0].date.year(), 2024);
        assert_eq!(taxable[0].amount, dec!(25.00)); // 500*(0.95-0.90)
        assert_eq!(taxable[1].date.year(), 2025);
        assert_eq!(taxable[1].amount, dec!(50.00)); // 500*(1.00-0.90)
    }

    /// A FOREX leg whose paired EUR amount carries the wrong (same) sign yields a negative rate and
    /// would fabricate a huge gain; it is refused instead.
    #[test]
    fn malformed_forex_same_sign_leg_is_rejected() {
        let flows = vec![
            // 1000 USD in, but the EUR leg is +910 (should be -910) → rate -910/1000 = -0.91.
            flow(4, "FOREX", dec!(1000), dec!(1000), Some(dec!(910))),
        ];
        let err = compute_fx_fifo(&flows, &[], rates(&[(4, dec!(0.90))])).unwrap_err();
        assert!(err.to_string().contains("non-positive"), "{err}");
    }
}
