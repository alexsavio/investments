use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::Duration;
use clap::{Arg, ArgAction, ArgMatches, value_parser};
use serde::Deserialize;
use serde::de::{Deserializer, Error, IgnoredAny};
use validator::Validate;

use crate::analysis::backtesting::config::BacktestingConfig;
use crate::analysis::performance::config::PerformanceMergingConfig;
use crate::broker_statement::{CorporateAction, Operation, SymbolRemappingRules};
use crate::brokers::Broker;
use crate::brokers::config::BrokersConfig;
use crate::cash_flow::config::deserialize_cash_flows;
use crate::core::{EmptyResult, GenericResult};
use crate::deposits::config::DepositConfig;
use crate::instruments::InstrumentInternalIds;
use crate::localities::{self, Country, Jurisdiction};
use crate::metrics::{self, config::MetricsConfig};
use crate::portfolio::config::AssetAllocationConfig;
use crate::quotes::QuotesConfig;
use crate::quotes::alphavantage::AlphaVantageConfig;
use crate::quotes::fcsapi::FcsApiConfig;
use crate::quotes::finnhub::FinnhubConfig;
use crate::quotes::twelvedata::TwelveDataConfig;
use crate::taxes::remapping::TaxRemappingConfig;
use crate::taxes::{
    self, TaxConfig, TaxExemption, TaxJurisdiction, TaxPaymentDay, TaxPaymentDaySpec, TaxRemapping,
};
use crate::telemetry::TelemetryConfig;
use crate::time;
use crate::types::{Date, Decimal};

pub struct CliConfig {
    pub log_level: log::Level,
    pub config_dir: PathBuf,
    pub cache_expire_time: Option<Duration>,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub db_path: String,
    #[serde(skip, default = "default_expire_time")]
    pub cache_expire_time: Duration,

    #[serde(default)]
    pub deposits: Vec<DepositConfig>,
    pub notify_deposit_closing_days: Option<u32>,

    #[serde(default)]
    pub portfolios: Vec<PortfolioConfig>,
    pub brokers: Option<BrokersConfig>,
    #[serde(default)]
    pub taxes: TaxConfig,

    #[serde(default)]
    #[validate(nested)]
    pub quotes: QuotesConfig,
    #[serde(default)]
    #[validate(nested)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    #[validate(nested)]
    pub backtesting: BacktestingConfig,

    #[serde(default)]
    pub telemetry: TelemetryConfig,

    // Deprecated
    pub alphavantage: Option<AlphaVantageConfig>,
    pub fcsapi: Option<FcsApiConfig>,
    pub finnhub: Option<FinnhubConfig>,
    pub twelvedata: Option<TwelveDataConfig>,

    #[serde(default, rename = "anchors")]
    _anchors: IgnoredAny,
}

impl Config {
    const DEFAULT_CONFIG_DIR_PATH: &str = "~/.investments";

    pub fn new<P: AsRef<Path>>(
        config_dir: P,
        cache_expire_time: Option<Duration>,
    ) -> GenericResult<Config> {
        let config_dir = config_dir.as_ref();

        let config_path = config_dir.join("config.yaml");
        let mut config = Config::load(&config_path)
            .map_err(|e| format!("Error while reading {config_path:?} configuration file: {e}"))?;

        if let Some(cache_expire_time) = cache_expire_time {
            config.cache_expire_time = cache_expire_time;
        }

        config_dir
            .join("db.sqlite")
            .to_str()
            .ok_or_else(|| format!("Invalid configuration directory path: {config_dir:?}"))?
            .clone_into(&mut config.db_path);

        Ok(config)
    }

    #[cfg(test)]
    pub fn mock() -> Config {
        Config {
            db_path: s!("/mock"),
            cache_expire_time: default_expire_time(),

            deposits: Vec::new(),
            notify_deposit_closing_days: None,

            portfolios: Vec::new(),
            brokers: None,
            taxes: Default::default(),

            quotes: Default::default(),
            metrics: Default::default(),
            backtesting: Default::default(),

            alphavantage: None,
            fcsapi: None,
            finnhub: None,
            twelvedata: None,
            telemetry: Default::default(),

            _anchors: Default::default(),
        }
    }

    pub fn args() -> [Arg; 3] {
        [
            Arg::new("config")
                .short('c')
                .long("config")
                .help(format!(
                    "Configuration directory path [default: {}]",
                    Self::DEFAULT_CONFIG_DIR_PATH
                ))
                .value_name("PATH")
                .value_parser(value_parser!(PathBuf)),
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Set verbosity level")
                .action(ArgAction::Count),
            Arg::new("cache_expire_time")
                .short('e')
                .long("cache-expire-time")
                .help("Quote cache expire time (in $number{m|h|d} format)")
                .value_name("DURATION")
                .value_parser(time::parse_duration),
        ]
    }

    pub fn parse_args(matches: &ArgMatches) -> GenericResult<CliConfig> {
        let log_level = match matches.get_count("verbose") {
            0 => log::Level::Info,
            1 => log::Level::Debug,
            2 => log::Level::Trace,
            _ => return Err!("Invalid verbosity level"),
        };

        let config_dir = matches.get_one("config").cloned().unwrap_or_else(|| {
            PathBuf::from(shellexpand::tilde(Self::DEFAULT_CONFIG_DIR_PATH).to_string())
        });

        let cache_expire_time = matches.get_one("cache_expire_time").cloned();

        Ok(CliConfig {
            log_level,
            config_dir,
            cache_expire_time,
        })
    }

    pub fn get_tax_country(&self) -> Country {
        match self.taxes.jurisdiction {
            Some(TaxJurisdiction::Germany) => localities::germany(&self.taxes),
            // Default to Russia when the jurisdiction is omitted (backwards compatibility).
            Some(TaxJurisdiction::Russia) | None => localities::russia(&self.taxes),
        }
    }

    pub fn get_portfolio(&self, name: &str) -> GenericResult<&PortfolioConfig> {
        for portfolio in &self.portfolios {
            if portfolio.name == name {
                return Ok(portfolio);
            }
        }

        Err!(
            "{:?} portfolio is not defined in the configuration file",
            name
        )
    }

    fn load(path: &Path) -> GenericResult<Config> {
        let mut config: Config = Config::read(path)?;

        config.validate()?;
        config.move_deprecated_settings();

        let mut portfolio_names = HashSet::new();

        for portfolio in &mut config.portfolios {
            if portfolio.name == metrics::PORTFOLIO_LABEL_ALL {
                return Err!(
                    "Invalid portfolio name: {:?}. The name is reserved",
                    portfolio.name
                );
            } else if !portfolio_names.insert(portfolio.name.clone()) {
                return Err!("Duplicate portfolio name: {:?}", portfolio.name);
            }

            portfolio.validate().map_err(|e| format!(
                "{:?} portfolio: {}", portfolio.name, e))?;
        }

        for deposit in &config.deposits {
            deposit.validate()?;
        }

        config.metrics.validate_inner(&portfolio_names)?;
        config.backtesting.validate_inner()?;

        Ok(config)
    }

    fn read(path: &Path) -> GenericResult<Config> {
        let mut data = Vec::new();
        File::open(path)?.read_to_end(&mut data)?;

        {
            // yaml-rust doesn't support merge key (https://github.com/chyh1990/yaml-rust/issues/68)

            use yaml_merge_keys::serde_yaml::{self as yaml, Value};

            let value: Value = yaml::from_slice(&data)?;
            let merged = yaml_merge_keys::merge_keys_serde(value.clone())?;
            if merged == value {
                return Ok(serde_yaml::from_slice(&data)?);
            }

            data.clear();
            yaml::to_writer(&mut data, &merged)?
        }

        Ok(serde_yaml::from_slice(&data).map_err(|err| {
            // To not confuse user with changed positions
            if let Some(message) = err.location().and_then(|location| {
                let message = err.to_string();
                let suffix = format!(" at line {} column {}", location.line(), location.column());
                message.strip_suffix(&suffix).map(ToOwned::to_owned)
            }) {
                return message;
            }

            err.to_string()
        })?)
    }

    fn move_deprecated_settings(&mut self) {
        if self.quotes.alphavantage.is_none() {
            if let Some(config) = self.alphavantage.take() {
                self.quotes.alphavantage.replace(config);
            }
        }

        if self.quotes.fcsapi.is_none() {
            if let Some(config) = self.fcsapi.take() {
                self.quotes.fcsapi.replace(config);
            }
        }

        if self.quotes.finnhub.is_none() {
            if let Some(config) = self.finnhub.take() {
                self.quotes.finnhub.replace(config);
            }
        }
    }
}

/// How a portfolio's foreign-currency balances are taxed under German law.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ForeignCurrencyTaxation {
    /// Interest-bearing cash (e.g. Interactive Brokers pays interest): currency gains are §20 EStG
    /// capital income (Abgeltungsteuer). The default — correct for the common broker case.
    #[default]
    InterestBearing,
    /// Non-interest-bearing currency held as an asset: currency gains are §23 EStG private
    /// Veräußerungsgeschäfte (one-year Spekulationsfrist, Anlage SO, personal income rate).
    NonInterestBearing,
}

/// A foreign-currency balance carried into the earliest statement, declared because the broker
/// export does not reach back to account opening. It seeds the German FX FIFO so a truncated-history
/// statement reconciles; the balance-chain check still validates the declared quantity against the
/// statement, so only the acquisition rate is trusted from config.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct OpeningForeignCurrency {
    /// Signed balance at the start of the earliest statement: positive = held (Guthaben), negative =
    /// borrowed (Kredit).
    pub quantity: Decimal,
    /// EUR value of one unit of the currency at acquisition (the rate the statement cannot supply).
    pub eur_per_unit: Decimal,
    /// Acquisition date (drives the §23 holding period). Accepts `YYYY.MM.DD` or `DD.MM.YYYY`.
    #[serde(deserialize_with = "deserialize_user_date")]
    pub as_of: Date,
}

fn deserialize_user_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    time::parse_user_date(&raw).map_err(D::Error::custom)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioConfig {
    pub name: String,
    pub broker: Broker,
    pub plan: Option<String>,

    #[serde(default, deserialize_with = "deserialize_optional_path")]
    pub statements: Option<PathBuf>,
    pub operations: Option<Vec<Operation>>,

    #[serde(default)]
    pub symbol_remapping: SymbolRemappingRules,
    #[serde(default, deserialize_with = "InstrumentInternalIds::deserialize")]
    pub instrument_internal_ids: InstrumentInternalIds,
    #[serde(default)]
    pub instrument_names: HashMap<String, String>,
    #[serde(default)]
    tax_remapping: Vec<TaxRemappingConfig>,
    #[serde(default)]
    pub corporate_actions: Vec<CorporateAction>,

    pub currency: Option<String>,
    pub min_trade_volume: Option<Decimal>,
    pub min_cash_assets: Option<Decimal>,
    pub restrict_buying: Option<bool>,
    pub restrict_selling: Option<bool>,

    #[serde(default)]
    pub merge_performance: PerformanceMergingConfig,

    #[serde(default)]
    pub assets: Vec<AssetAllocationConfig>,

    #[serde(
        default,
        rename = "tax_payment_day",
        deserialize_with = "TaxPaymentDaySpec::deserialize"
    )]
    tax_payment_day_spec: TaxPaymentDaySpec,

    #[serde(default)]
    pub tax_exemptions: Vec<TaxExemption>,

    #[serde(default, deserialize_with = "deserialize_cash_flows")]
    pub tax_deductions: Vec<(Date, Decimal)>,

    /// German tax treatment of this account's foreign-currency balances (§20 vs §23).
    #[serde(default)]
    pub foreign_currency_taxation: ForeignCurrencyTaxation,

    /// Foreign-currency balances carried into the earliest statement, keyed by ISO 4217 code. Only
    /// needed when the broker export does not begin from a zero balance at account opening; the
    /// German FX FIFO otherwise requires and validates a full-history ledger.
    #[serde(default)]
    pub opening_foreign_currency: BTreeMap<String, OpeningForeignCurrency>,
}

impl PortfolioConfig {
    pub fn currency(&self) -> &str {
        self.currency
            .as_deref()
            .unwrap_or_else(|| self.broker.jurisdiction().traits().currency)
    }

    pub fn has_statement(&self) -> bool {
        match self.broker {
            Broker::Other => self.operations.is_some(),
            _ => self.statements.is_some(),
        }
    }

    pub fn get_stock_symbols(&self) -> HashSet<String> {
        let mut symbols = HashSet::new();

        for asset in &self.assets {
            asset.get_stock_symbols(&mut symbols);
        }

        symbols
    }

    pub fn tax_payment_day(&self) -> TaxPaymentDay {
        TaxPaymentDay::new(self.broker.jurisdiction(), self.tax_payment_day_spec)
    }

    pub fn get_tax_remapping(&self) -> GenericResult<TaxRemapping> {
        let mut remapping = TaxRemapping::new();

        for config in &self.tax_remapping {
            remapping.add(config.date, &config.description, config.to_date)?;
        }

        Ok(remapping)
    }

    pub fn close_date() -> Date {
        time::today()
    }

    fn validate(&self) -> EmptyResult {
        let currency = self.currency();

        match currency {
            "RUB" | "USD" | "EUR" => (),
            _ => return Err!("Unsupported portfolio currency: {currency}"),
        }

        if self.statements.is_some() && self.broker == Broker::Other {
            return Err!("Statements path specification is not supported for broker {:?}", Broker::Other.id());
        } else if self.operations.is_some() && self.broker != Broker::Other {
            return Err!("Portfolio operations specification is only supported for broker {:?}", Broker::Other.id());
        }

        if
            matches!(self.tax_payment_day_spec, TaxPaymentDaySpec::OnClose(_)) &&
            self.broker.jurisdiction() != Jurisdiction::Russia
        {
            return Err!(
                "On close tax payment date is only available for brokers with Russia jurisdiction"
            );
        }

        taxes::validate_tax_exemptions(self.broker, &self.tax_exemptions)?;

        Ok(())
    }
}

fn default_expire_time() -> Duration {
    Duration::minutes(1)
}

fn deserialize_optional_path<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
    where D: Deserializer<'de>
{
    let path: Option<String> = Deserialize::deserialize(deserializer)?;
    path.as_deref().map(parse_path::<D>).transpose()
}

fn parse_path<'de, D>(path: &str) -> Result<PathBuf, D::Error>
    where D: Deserializer<'de>
{
    let path = PathBuf::from(shellexpand::tilde(path).to_string());
    if !path.is_absolute() {
        return Err(D::Error::custom("The path must be absolute"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_foreign_currency_parses_signed_lots_and_user_dates() {
        let config: PortfolioConfig = serde_yaml::from_str(
            "name: test\n\
             broker: interactive-brokers\n\
             foreign_currency_taxation: non_interest_bearing\n\
             opening_foreign_currency:\n  \
               USD:\n    \
                 quantity: '90.00'\n    \
                 eur_per_unit: '0.92'\n    \
                 as_of: '2023.11.04'\n  \
               GBP:\n    \
                 quantity: '-12.00'\n    \
                 eur_per_unit: '1.15'\n    \
                 as_of: '01.09.2023'\n",
        )
        .unwrap();

        assert_eq!(
            config.foreign_currency_taxation,
            ForeignCurrencyTaxation::NonInterestBearing
        );

        let usd = &config.opening_foreign_currency["USD"];
        assert_eq!(usd.quantity, dec!(90.00));
        assert_eq!(usd.eur_per_unit, dec!(0.92));
        assert_eq!(usd.as_of, Date::from_ymd_opt(2023, 11, 4).unwrap());

        // A borrowed opening (negative) and the DD.MM.YYYY date form both parse.
        let gbp = &config.opening_foreign_currency["GBP"];
        assert_eq!(gbp.quantity, dec!(-12.00));
        assert_eq!(gbp.as_of, Date::from_ymd_opt(2023, 9, 1).unwrap());
    }

    #[test]
    fn opening_foreign_currency_defaults_to_empty() {
        let config: PortfolioConfig =
            serde_yaml::from_str("name: test\nbroker: interactive-brokers\n").unwrap();
        assert!(config.opening_foreign_currency.is_empty());
        assert_eq!(
            config.foreign_currency_taxation,
            ForeignCurrencyTaxation::InterestBearing
        );
    }
}
