use std::collections::BTreeMap;
use std::rc::Rc;

use chrono::{Datelike, Duration};

use crate::currency::Cash;
use crate::exchanges::Exchange;
use crate::taxes::spain::scale::SavingsScale;
use crate::taxes::{FixedTaxRate, ProgressiveTaxRate, TaxConfig, TaxRate};
use crate::types::{Date, Decimal};

#[derive(Clone)]
pub struct Country {
    pub jurisdiction: Jurisdiction,
    pub currency: &'static str,

    tax_rates: Rc<BTreeMap<i32, Box<dyn TaxRate>>>,
    tax_agent_rates: Rc<BTreeMap<i32, Box<dyn TaxRate>>>,

    // When foreign broker withholds tax from dividends, Russian investors can credit this withheld tax due to the
    // agreement on avoidance of double taxation.
    //
    // Until 2024 foreign withheld tax was limited by 10%, but starting from mid 2024 it may be up to 30%. Though
    // theoretically it's possible to credit it (https://sergeynaumov.com/offset-of-tax-on-dividends/), there is a
    // problem with crediting anything more than 13% (up to effective progressive tax rate) because of the current tax
    // calculation scheme in Russia:
    // 1. Individual sends tax form in which all income is calculated with base 13% tax rate (and there is no way to
    //    credit more than 13%).
    // 2. At the end of the year tax authority sums up all the income and issues a tax invoice for paying > 13% tax.
    // ... and we have no ability to credit our > 13% here. So we take this limitation into account via this option.
    pub tax_credit_rate_limit: Option<Decimal>,
}

impl Country {
    fn new(
        jurisdiction: Jurisdiction,
        tax_rates: BTreeMap<i32, Box<dyn TaxRate>>,
        tax_agent_rates: BTreeMap<i32, Box<dyn TaxRate>>,
        tax_credit_rate_limit: Option<Decimal>,
    ) -> Country {
        Country {
            jurisdiction,
            currency: jurisdiction.traits().currency,

            tax_rates: Rc::new(tax_rates),
            tax_agent_rates: Rc::new(tax_agent_rates),
            tax_credit_rate_limit,
        }
    }

    pub fn cash(&self, amount: Decimal) -> Cash {
        Cash::new(self.currency, amount)
    }

    pub fn tax_rate(&self, year: i32) -> Box<dyn TaxRate> {
        self.tax_rates.range(..=year).last().unwrap().1.clone()
    }

    pub fn tax_agent_rate(&self, year: i32) -> Box<dyn TaxRate> {
        self.tax_agent_rates
            .range(..=year)
            .last()
            .unwrap()
            .1
            .clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Jurisdiction {
    Russia,
    Usa,
    Germany,
    Spain,
}

pub struct JurisdictionTraits {
    pub name: &'static str,
    pub code: &'static str,
    pub currency: &'static str,
    pub tax_precision: u32,

    /// Official source of currency rates for this jurisdiction's tax authority.
    ///
    /// Not a preference: the authority dictates which rates a return must use, so this is
    /// derived from the jurisdiction and never configured. Deciding it per call site is how
    /// `simulate-sell` came to convert a German filer's amounts at Central Bank of Russia
    /// rates while `tax-statement` used the ECB on the same positions.
    pub rate_source: RateSourceKind,
}

/// Which central bank publishes the reference rates a jurisdiction's tax authority requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateSourceKind {
    /// Central Bank of Russia. RUB base.
    Cbr,
    /// European Central Bank. EUR base.
    Ecb,
}

impl Jurisdiction {
    pub fn traits(self) -> JurisdictionTraits {
        match self {
            Jurisdiction::Russia => JurisdictionTraits {
                rate_source: RateSourceKind::Cbr,
                name: "Russia",
                code: "RU",
                currency: "RUB",
                tax_precision: 0,
            },
            Jurisdiction::Usa => JurisdictionTraits {
                rate_source: RateSourceKind::Cbr,
                name: "USA",
                code: "US",
                currency: "USD",
                tax_precision: 2,
            },
            Jurisdiction::Germany => JurisdictionTraits {
                rate_source: RateSourceKind::Ecb,
                name: "Germany",
                code: "DE",
                currency: "EUR",
                tax_precision: 2,
            },
            Jurisdiction::Spain => JurisdictionTraits {
                // Both the state and the foral tax administrations accept ECB reference rates for
                // converting foreign-currency income, so Spain shares Germany's rate source.
                rate_source: RateSourceKind::Ecb,
                name: "Spain",
                code: "ES",
                currency: "EUR",
                tax_precision: 2,
            },
        }
    }
}

pub fn russia(config: &TaxConfig) -> Country {
    let jurisdiction = Jurisdiction::Russia;
    let tax_precision = jurisdiction.traits().tax_precision;

    // Starting from 2021 we had progressive tax rate with single tax base
    let rates_2021 = Rc::new(btreemap! {
        dec!(0) => dec!(0.13),
        dec!(5_000_000) => dec!(0.15),
    });

    // Starting from 2025 we've got progressive tax rate with two tax bases:
    // 1. Income from employment
    // 2. Dividends, interest, trading, property
    let rates_2025 = Rc::new(btreemap! {
        dec!(0) => dec!(0.13),
        dec!(2_400_000) => dec!(0.15),
    });

    let tax_agent_calculators = btreemap! {
        i32::MIN => Box::new(FixedTaxRate::new(dec!(0.13), tax_precision)) as Box<dyn TaxRate>,
        2021 => Box::new(ProgressiveTaxRate::new(dec!(0), rates_2021.clone(), tax_precision)) as Box<dyn TaxRate>,
        2025 => Box::new(ProgressiveTaxRate::new(dec!(0), rates_2025.clone(), tax_precision)) as Box<dyn TaxRate>,
    };

    let mut tax_calculators = tax_agent_calculators.clone();

    for (&year, &income) in config.income.range(2021..2025) {
        let calc = Box::new(ProgressiveTaxRate::new(
            income,
            rates_2021.clone(),
            tax_precision,
        ));
        tax_calculators.insert(year, calc);
    }

    Country::new(
        Jurisdiction::Russia,
        tax_calculators,
        tax_agent_calculators,
        Some(dec!(0.13)),
    )
}

pub fn germany(_config: &TaxConfig) -> Country {
    let jurisdiction = Jurisdiction::Germany;
    let tax_precision = jurisdiction.traits().tax_precision;

    // Approximation only. This flat 26.375% FixedTaxRate (25% Abgeltungssteuer + 5.5%
    // Solidaritätszuschlag) is for the analysis/rebalancing views: it has no church tax, no
    // Teilfreistellung, no loss pots, and no Sparer-Pauschbetrag. The real German tax math lives in
    // the `tax_statement::germany` module — do not use this rate for the filing statement.
    // Church tax (8% or 9%) is configured per user, not included in this base rate.
    //
    // Nor to price a sale: what a disposal costs depends on the whole tax year, so it is not a rate
    // times a gain. `tax_statement::germany::compute_tax_year` computes the year and
    // `analysis::sell_simulation` prices a hypothetical sale at the difference it makes to it —
    // which is how a gain lands tax-free while the year's §20(6) Aktien pot is still negative,
    // where this rate would invent a charge.

    let abgeltungssteuer_rate = dec!(0.25);
    let solidarity_surcharge = dec!(0.055); // 5.5% of Abgeltungssteuer

    // Base tax rate without church tax
    let effective_base_rate = abgeltungssteuer_rate * (dec!(1) + solidarity_surcharge);

    let tax_calculators = btreemap! {
        2009 => Box::new(FixedTaxRate::new(effective_base_rate, tax_precision)) as Box<dyn TaxRate>,
    };

    // Germany doesn't have tax agents for foreign accounts, so use same rates
    let tax_agent_calculators = tax_calculators.clone();

    // No tax credit limit for Germany (foreign tax credits handled differently)
    Country::new(
        Jurisdiction::Germany,
        tax_calculators,
        tax_agent_calculators,
        None,
    )
}

pub fn spain(config: &TaxConfig) -> Country {
    let jurisdiction = Jurisdiction::Spain;
    let tax_precision = jurisdiction.traits().tax_precision;

    // Approximation only. These are the real per-year savings-base scales (escala de la base
    // liquidable del ahorro), but the analysis/rebalancing views are all they are for: there is no
    // compensation between the RCM and ganancias groups, no 4-year loss ledgers, no Gipuzkoa
    // actualization coefficients, and no valores-homogéneos loss deferral. The real Spanish tax
    // math lives in the `tax_statement::spain` module — do not use this rate for the filing
    // statement.
    //
    // Nor to price a sale: what a disposal costs depends on the whole tax year, so it is not a rate
    // times a gain. `tax_statement::spain::compute_tax_year` computes the year and
    // `analysis::sell_simulation` prices a hypothetical sale at the difference it makes to it —
    // which is how a loss booked two months after a repurchase correctly prices at zero deduction,
    // where this rate would invent one.
    let regime = config.spanish_regime();

    let mut tax_calculators: BTreeMap<i32, Box<dyn TaxRate>> = BTreeMap::new();
    let mut previous: Option<Vec<(Decimal, Decimal)>> = None;

    for year in SavingsScale::FIRST_YEAR..=SavingsScale::LAST_YEAR {
        let scale = SavingsScale::for_year(regime, year)
            .expect("every year in the shipped range resolves to a scale");
        let brackets = scale.brackets().to_vec();

        // Consecutive years share a scale far more often than not, so only the years that actually
        // change get an entry; `Country::tax_rate` resolves the rest by range lookup.
        if previous.as_ref() == Some(&brackets) {
            continue;
        }

        // The earliest shipped scale is keyed at `i32::MIN` rather than at its own year so that
        // `Country::tax_rate` still answers for years before the shipped range. It takes the last
        // entry at or below the queried year and unwraps, so an empty range would panic.
        let key = if previous.is_none() { i32::MIN } else { year };
        let rates = Rc::new(brackets.iter().copied().collect::<BTreeMap<_, _>>());
        tax_calculators.insert(
            key,
            Box::new(ProgressiveTaxRate::new(dec!(0), rates, tax_precision)) as Box<dyn TaxRate>,
        );
        previous = Some(brackets);
    }

    // A foreign broker withholds no Spanish tax, so there is no tax-agent rate to distinguish.
    let tax_agent_calculators = tax_calculators.clone();

    // The double-taxation credit (NF 3/2014 art. 91 / LIRPF art. 80) is capped by the average
    // savings rate, which is a year-level figure this per-rate abstraction cannot express. The
    // statement module applies it; there is no flat limit to declare here.
    Country::new(
        Jurisdiction::Spain,
        tax_calculators,
        tax_agent_calculators,
        None,
    )
}

pub fn get_russian_central_bank_min_last_working_day(today: Date) -> Date {
    // New Year holidays
    if today.month() == 1 && today.day() <= 12 {
        std::cmp::max(today - Duration::days(11), date!(today.year() - 1, 12, 29))
    // COVID-19 pandemic
    } else if today.year() == 2020 && today.month() == 4 && today.day() <= 6 {
        date!(2020, 3, 28)
    // Weekends, 8 March, May and occasional COVID-19 pandemic holidays
    } else {
        today - Duration::days(5)
    }
}

pub fn get_nearest_possible_russian_account_close_date() -> Date {
    [Exchange::Moex, Exchange::Spb]
        .iter()
        .map(|exchange| {
            let execution_date = exchange
                .trading_mode()
                .execution_date(crate::exchanges::today_trade_conclusion_time());

            let mut close_date = execution_date;
            while exchange.min_last_working_day(close_date) < execution_date {
                close_date += Duration::days(1);
            }

            close_date
        })
        .max()
        .unwrap()
}

pub fn us_dividend_tax_rate(date: Date) -> Decimal {
    if date >= date!(2024, 8, 16) {
        dec!(0.3)
    } else {
        dec!(0.1)
    }
}

pub fn deduce_us_dividend_amount(date: Date, result_income: Cash) -> Cash {
    let tax_rate = us_dividend_tax_rate(date);
    (result_income / (dec!(1) - tax_rate)).round()
}

#[cfg(test)]
mod tests {
    use crate::taxes::spain::SpanishTaxRegime;
    use crate::taxes::{IncomeType, SpanishTaxConfig};

    use super::*;

    fn spain_config(regime: SpanishTaxRegime) -> TaxConfig {
        TaxConfig {
            spain: Some(SpanishTaxConfig {
                regime,
                loss_carryforward: Default::default(),
                deferred_losses: Vec::new(),
                coefficients: Default::default(),
            }),
            ..Default::default()
        }
    }

    /// Spain files in EUR and its tax authorities take ECB reference rates, which is what makes
    /// `CurrencyConverter::for_jurisdiction` hand the Spanish flow the ECB backend rather than the
    /// Central Bank of Russia one used for the Russia/USA path.
    #[test]
    fn spain_country_traits() {
        let traits = Jurisdiction::Spain.traits();
        assert_eq!(traits.name, "Spain");
        assert_eq!(traits.code, "ES");
        assert_eq!(traits.currency, "EUR");
        assert_eq!(traits.tax_precision, 2);
        assert_eq!(traits.rate_source, RateSourceKind::Ecb);

        let country = spain(&TaxConfig::default());
        // `Jurisdiction` is not `Debug`, so compare rather than `assert_eq!`.
        assert!(country.jurisdiction == Jurisdiction::Spain);
        assert_eq!(country.currency, "EUR");
        // The double-taxation credit cap is a year-level average rate, not a flat limit.
        assert!(country.tax_credit_rate_limit.is_none());
    }

    /// The approximation must track the configured regime: on €10,000 of savings income the 2026
    /// Gipuzkoa scale charges 1,925 (7,500×19% + 2,500×20%) while the state scale charges 1,980
    /// (6,000×19% + 4,000×21%). Reading the wrong regime would silently misprice every analysis
    /// view by the difference.
    #[test]
    fn spain_approximation_follows_the_configured_regime() {
        let tax = |regime| {
            spain(&spain_config(regime))
                .tax_rate(2026)
                .tax(IncomeType::Trading, dec!(10000))
        };

        assert_eq!(tax(SpanishTaxRegime::Gipuzkoa), dec!(1925));
        assert_eq!(tax(SpanishTaxRegime::Comun), dec!(1980));
        // TRLFIRPF art. 60: 6,000×20% + 4,000×22%, the statute's own cuota at €10,000.
        assert_eq!(tax(SpanishTaxRegime::Navarra), dec!(2080));
        // No `taxes.spain` block: the analysis views still get a rate (Gipuzkoa default) rather
        // than failing. The filing path errors instead of guessing.
        assert_eq!(
            spain(&TaxConfig::default())
                .tax_rate(2026)
                .tax(IncomeType::Trading, dec!(10000)),
            dec!(1925)
        );
    }

    /// The pre-2026 foral scale must not be shadowed by the NF 1/2025 reform, and a year before the
    /// shipped range must still resolve instead of panicking on an empty range lookup.
    #[test]
    fn spain_approximation_is_year_sensitive() {
        let country = spain(&TaxConfig::default());

        // 2025 Gipuzkoa: 2,500×20% + 7,500×21% = 500 + 1,575 = 2,075.
        assert_eq!(
            country.tax_rate(2025).tax(IncomeType::Trading, dec!(10000)),
            dec!(2075)
        );
        // 2026 applies the reformed scale.
        assert_eq!(
            country.tax_rate(2026).tax(IncomeType::Trading, dec!(10000)),
            dec!(1925)
        );
        // Before the shipped range the earliest scale answers rather than panicking.
        assert_eq!(
            country.tax_rate(2020).tax(IncomeType::Trading, dec!(10000)),
            dec!(2075)
        );
    }

    /// The state top bracket rose from 28% to 30% in 2025, so the approximation must not reuse one
    /// year's scale for the other above €300,000.
    #[test]
    fn spain_comun_approximation_tracks_the_top_bracket_change() {
        let country = spain(&spain_config(SpanishTaxRegime::Comun));

        // 400,000: 71,880 cumulative at 300,000, then 100,000 at the top rate.
        assert_eq!(
            country.tax_rate(2024).tax(IncomeType::Trading, dec!(400000)),
            dec!(99880)
        );
        assert_eq!(
            country.tax_rate(2025).tax(IncomeType::Trading, dec!(400000)),
            dec!(101880)
        );
    }
}
