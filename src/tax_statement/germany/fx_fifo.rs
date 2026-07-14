//! Per-currency signed-inventory FIFO for German Fremdwährungsgewinne (§20 EStG).
//!
//! Foreign-currency cash is replayed in statement order. A positive running balance is a
//! Fremdwährungsguthaben; because IBKR pays interest on cash balances it is an interest-bearing
//! account, so its gains and losses fall under §20 EStG (Abgeltungsteuer). A negative balance is a
//! Fremdwährungskredit whose repayment FX result is not taxable (Tilgung eines
//! Fremdwährungskredits, BMF 19.05.2022 Rz. 131).
//!
//! Each disposal of held currency (a securities purchase, a fee, a reconversion to EUR) realizes a
//! §20 gain or loss; each repayment of borrowed currency realizes a non-taxable result. Real
//! currency exchanges are valued at the actual execution rate (the paired EUR leg); every other
//! movement — including securities bought directly in the foreign currency ("verdeckte" gains) — is
//! valued at the ECB reference rate of the transaction date.

use std::collections::{HashMap, VecDeque};

use crate::broker_statement::ForeignCashFlow;
use crate::core::GenericResult;
use crate::time::Date;
use crate::types::Decimal;

/// Realized FX result for one foreign currency over the statement.
#[derive(Debug)]
pub struct CurrencyFxResult {
    pub currency: String,
    /// §20-taxable disposals of held currency (Fremdwährungsguthaben).
    pub taxable: Vec<FxRealization>,
    /// Total non-taxable Fremdwährungskredit-Tilgung result in EUR (full precision).
    pub non_taxable: Decimal,
}

/// A single realized taxable FX gain/loss in EUR (full precision; positive = gain, negative = loss).
#[derive(Debug)]
pub struct FxRealization {
    pub date: Date,
    pub amount: Decimal,
    pub activity_code: String,
}

/// One FIFO lot: a signed quantity of the foreign currency carrying its EUR-per-unit rate.
struct Lot {
    /// Remaining quantity: positive = held (Guthaben), negative = borrowed (Kredit).
    qty: Decimal,
    /// EUR value of one unit of the foreign currency at acquisition.
    rate: Decimal,
}

/// Replay the cash-flow ledger through a per-currency signed-inventory FIFO.
///
/// `ecb_rate(date, currency)` returns the ECB reference rate as EUR per one unit of `currency`.
/// EUR movements are the filing currency (no FX gain against themselves) and are skipped. Results
/// are returned in first-seen currency order.
pub fn compute_fx_fifo<R>(
    flows: &[ForeignCashFlow],
    ecb_rate: R,
) -> GenericResult<Vec<CurrencyFxResult>>
where
    R: Fn(Date, &str) -> GenericResult<Decimal>,
{
    let mut order: Vec<String> = Vec::new();
    let mut lots: HashMap<String, VecDeque<Lot>> = HashMap::new();
    let mut results: HashMap<String, CurrencyFxResult> = HashMap::new();
    let mut running: HashMap<String, Decimal> = HashMap::new();

    for flow in flows {
        if flow.currency == "EUR" || flow.amount == dec!(0) {
            continue;
        }

        // The FIFO can only value what it replays, and it starts every currency from an empty
        // inventory. Verify that against the statement's own running balance: the movements must
        // sum, in document order from a zero opening balance, to the reported balance after each
        // step. A mismatch means a carried-in balance (foreign cash held across the year boundary,
        // whose prior-year acquisition rate this statement does not carry), out-of-order rows, or
        // dropped rows — each of which would silently mis-classify §20 gains, so refuse rather than
        // guess a tax figure.
        let running_balance = running.entry(flow.currency.clone()).or_insert(dec!(0));
        *running_balance += flow.amount;
        if (*running_balance - flow.balance).abs() > dec!(0.01) {
            return Err(format!(
                "Foreign-currency ledger for {} is inconsistent on {}: movements sum to {} but the \
                 statement balance is {}. The Statement of Funds must start from a zero {} balance \
                 and be in document order; carried-in balances and out-of-order rows are not \
                 supported.",
                flow.currency, flow.date, running_balance, flow.balance, flow.currency
            )
            .into());
        }

        // EUR value per unit of the currency for this movement: the actual execution rate for a
        // real exchange (the paired EUR leg, which carries the opposite sign), otherwise the ECB
        // reference rate.
        let rate = match (flow.activity_code.as_str(), flow.eur_execution) {
            ("FOREX", Some(eur_execution)) => -eur_execution / flow.amount,
            _ => ecb_rate(flow.date, &flow.currency)?,
        };

        if !results.contains_key(&flow.currency) {
            order.push(flow.currency.clone());
            lots.insert(flow.currency.clone(), VecDeque::new());
            results.insert(
                flow.currency.clone(),
                CurrencyFxResult {
                    currency: flow.currency.clone(),
                    taxable: Vec::new(),
                    non_taxable: dec!(0),
                },
            );
        }
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
                });
                remaining = dec!(0);
                continue;
            }

            let front = lots.front_mut().unwrap();
            let units = front.qty.abs().min(remaining.abs());
            if front.qty.is_sign_positive() {
                // Disposing held currency → §20 taxable gain/loss.
                result.taxable.push(FxRealization {
                    date: flow.date,
                    amount: units * (rate - front.rate),
                    activity_code: flow.activity_code.clone(),
                });
            } else {
                // Repaying borrowed currency → non-taxable (Tilgung Fremdwährungskredit).
                result.non_taxable += units * (front.rate - rate);
            }
            front.qty -= sign(front.qty) * units;
            remaining -= sign(remaining) * units;
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
        let results = compute_fx_fifo(&flows, rates(&[(4, dec!(0.90))])).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].taxable.is_empty());
        // Repaid 1000 USD borrowed at 0.90 with USD costing 0.91 → 1000*(0.90-0.91) = -10.
        assert_eq!(results[0].non_taxable, dec!(-10));
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
        let results = compute_fx_fifo(&flows, rates(&[(13, dec!(0.8607))])).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].non_taxable, dec!(0));
        assert_eq!(results[0].taxable.len(), 1);
        // 1000*(0.8607-0.8588) = 1.90.
        assert_eq!(results[0].taxable[0].amount, dec!(1.90));
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
        let results = compute_fx_fifo(&flows, rates(&[(1, dec!(0.86))])).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].non_taxable, dec!(0));
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
        let err = compute_fx_fifo(&flows, rates(&[(4, dec!(0.90))])).unwrap_err();
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
        let err = compute_fx_fifo(&flows, rates(&[(13, dec!(0.8607))])).unwrap_err();
        assert!(err.to_string().contains("inconsistent"), "{err}");
    }
}
