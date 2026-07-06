use std::collections::HashMap;

use crate::core::GenericResult;
use crate::currency::Cash;
use crate::localities::{Country, Jurisdiction};
use crate::taxes::{self, IncomeType, TaxRate};
use crate::types::Decimal;

pub struct Tax {
    pub expected: Cash,
    pub withheld: Cash,
    pub to_pay: Cash,

    // The amount by which the tax was reduced due to:
    // * Trading: various tax deductions
    // * Dividends: taking into account already withheld tax
    pub deduction: Cash,
}

pub struct TaxWithheld {
    pub amount: Cash,
    pub credit_rate_limit: Option<Decimal>,
}

pub struct TaxCalculator {
    pub country: Country,
    years: HashMap<i32, Box<dyn TaxRate>>,
}

impl TaxCalculator {
    pub fn new(country: Country) -> TaxCalculator {
        TaxCalculator {
            country,
            years: HashMap::new(),
        }
    }

    // Attention: Modifies calculator state. Must be called only for income that won't be decreased later via deductions
    // or looses balancing.
    pub fn tax_income(&mut self, income_type: IncomeType, year: i32, income: Cash, tax_withheld: Option<TaxWithheld>) -> Tax {
        calculate(self.country.jurisdiction, self.year(year), income_type, income, tax_withheld)
    }

    // Intended for dividends, tax for which was withheld by tax agent.
    pub fn tax_agent_income(&mut self, income_type: IncomeType, year: i32, income: Cash, mut tax_withheld: Cash) -> GenericResult<Tax> {
        if tax_withheld.currency != self.country.currency {
            return Err!("Got withheld tax in an unexpected currency: {}", tax_withheld.currency)
        }

        let orig_tax_withheld = tax_withheld;
        tax_withheld.amount = taxes::round_tax(tax_withheld.amount, self.country.jurisdiction.traits().tax_precision);

        if orig_tax_withheld != tax_withheld {
            return Err!("Got an unexpected withheld tax: {orig_tax_withheld} vs {tax_withheld}");
        }

        // Please note the following:
        //
        // 1. Withheld tax may be less than 13%. It may be even zero if company distributes dividends from other
        // companies for which tax has been already withheld, so we should trust the provided amount.
        //
        // See https://web.archive.org/web/20240622133328/https://smart-lab.ru/company/tinkoff_invest/blog/631922.php
        // for details.
        //
        // 2. In case of progressive tax rates the withheld tax may be calculated using lower tax rate, as broker
        // doesn't know total client's income. We try to workaround the case: tax the income using lowest tax rate and
        // if the result is equal to or less than the withheld tax, assume that there is no special case here, so we can
        // tax the dividend using our calculator which are aware of actual total tax base.

        let tax = self.tax_income(income_type, year, income, Some(TaxWithheld {
            amount: tax_withheld,
            credit_rate_limit: None,
        }));

        // This call increases total tax base which we should do in both cases
        let lowest_tax = Cash::new(income.currency, self.country.tax_agent_rate(year).tax(income_type, income.amount));
        if tax_withheld < lowest_tax || tax_withheld > tax.expected {
            return Ok(Tax {
                expected: tax_withheld,
                withheld: tax_withheld,
                deduction: tax_withheld,
                to_pay: Cash::zero(self.country.currency),
            });
        }

        Ok(tax)
    }

    // Attention: Modifies calculator state. Must be called only for income that won't be decreased later via deductions
    // or looses balancing.
    pub fn tax_deductible_income(&mut self, income_type: IncomeType, year: i32, income: Cash, taxable_income: Cash) -> Tax {
        let country = self.country.jurisdiction;

        let calc = self.year(year);
        let mut dry_run_calc = calc.clone();

        let full = calculate(country, &mut dry_run_calc, income_type, income, None);
        let real = calculate(country, calc, income_type, taxable_income, None);

        assert!(real.withheld.is_zero());
        assert_eq!(real.to_pay, real.expected);
        assert!(real.expected <= full.expected);

        Tax {
            expected: full.expected,
            withheld: real.withheld,
            deduction: full.expected - real.to_pay,
            to_pay: real.to_pay,
        }
    }

    // Attention: Always operates on clean calculator state. Intended for intermediate calculations during stock selling
    // processing which are processed before any looses balancing.
    pub fn tax_deductible_income_dry_run(&self, income_type: IncomeType, year: i32, income: Cash, taxable_income: Cash) -> Tax {
        let country = self.country.jurisdiction;

        let mut full_calc = self.country.tax_rate(year);
        let mut real_calc = full_calc.clone();

        let full = calculate(country, &mut full_calc, income_type, income, None);
        let real = calculate(country, &mut real_calc, income_type, taxable_income, None);

        assert!(real.withheld.is_zero());
        assert_eq!(real.to_pay, real.expected);
        assert!(real.expected <= full.expected);

        Tax {
            expected: full.expected,
            withheld: real.withheld,
            deduction: full.expected - real.to_pay,
            to_pay: real.to_pay,
        }
    }

    fn year(&mut self, year: i32) -> &mut Box<dyn TaxRate> {
        self.years.entry(year).or_insert_with(|| self.country.tax_rate(year))
    }
}

fn calculate(
    jurisdiction: Jurisdiction, calc: &mut Box<dyn TaxRate>,
    income_type: IncomeType, income: Cash, tax_withheld: Option<TaxWithheld>,
) -> Tax {
    let country = jurisdiction.traits();

    assert_eq!(income.currency, country.currency);
    let expected = calc.tax(income_type, income.amount);

    let (withheld, to_pay) = if let Some(tax_withheld) = tax_withheld {
        assert!(!tax_withheld.amount.is_negative());
        assert_eq!(tax_withheld.amount.currency, country.currency);

        let mut credited_tax = tax_withheld.amount.amount;
        if let Some(credit_rate_limit) = tax_withheld.credit_rate_limit {
            let credited_tax_limit = std::cmp::max(dec!(0), income.amount * credit_rate_limit);
            credited_tax = std::cmp::min(credited_tax, credited_tax_limit);
        }

        (
            tax_withheld.amount.amount,
            std::cmp::max(dec!(0), expected - taxes::round_tax(credited_tax, country.tax_precision))
        )
    } else {
        (dec!(0), expected)
    };

    Tax {
        expected: Cash::new(country.currency, expected),
        withheld: Cash::new(country.currency, withheld),
        deduction: Cash::new(country.currency, expected - to_pay),
        to_pay: Cash::new(country.currency, to_pay),
    }
}