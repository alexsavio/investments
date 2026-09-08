[![Test status](https://github.com/KonishchevDmitry/investments/actions/workflows/test.yml/badge.svg)](https://github.com/KonishchevDmitry/investments/actions/workflows/test.yml)

# Investments

Helps you with managing your investments:

* **Analysis:** calculates average rate of return from cash investments by comparing portfolio performance to performance of a bank deposit in USD and RUB currency with exactly the same investments and monthly capitalization. Considers taxes, commissions, dividends, tax deductions and optionally inflation when calculates portfolio performance.
* **Backtesting:** backtests you portfolio against configured benchmarks.
* **Portfolio rebalancing:** instructs you which orders you have to submit to make your portfolio in order with your asset allocation.
* **Stock selling simulation:** calculates revenue, profit, taxes and real profit percent which considers taxes into account.
* **Automatic tax statement generation:** reads broker statements and generates tax reports:
  * **Russia:** Alters *.deX file (created by Russian tax program named Декларация) by adding all required information about income from stock selling, paid dividends and idle cash interest.
  * **Germany:** Generates CSV tax reports with Abgeltungssteuer, Solidaritätszuschlag, Kirchensteuer calculations, Teilfreistellung for ETFs, and foreign tax credits. See [Germany tax documentation](docs/germany-taxes.md).
  * **Spain:** Generates CSV tax reports and a printable A4 report for the IRPF savings base under the Gipuzkoa foral regime (Norma Foral 3/2014), Territorio Común (LIRPF) or Navarra (Decreto Foral Legislativo 4/2008), with per-lot actualization coefficients, the valores-homogéneos deferral rule, four-year loss compensation, the double-taxation credit, and Navarra's own compensation order, 3% fee ceiling and €3,000 small-disposals exemption. See [Spain tax documentation](docs/spain-taxes.md).
* **Bank deposits control:** view opened bank deposits all in one place and get notified about upcoming deposit closures.
* **Metrics:** exports analysis results in Prometheus format.

Targeted for investors in Russia, Germany and Spain who use [Firstrade](https://www.firstrade.com/), [Interactive Brokers](https://interactivebrokers.com/), [БКС](https://broker.ru/), [Сбер](https://sberbank.ru/), [Т-Банк](https://www.tbank.ru/) or non-exchange-traded unit investment funds.

## Installation

See [installation instructions](docs/install.md).

## Configuration

Create `~/.investments/config.yaml` configuration file. See [example](docs/config-example.yaml) which contains typical configuration for each broker, tax exemptions that are applicable to the account and more. Don't forget to obtain API token for FCS API and Finnhub (see [stock and forex quotes providers](docs/quotes.md) for details).

## Usage

## Stocks

Investments is designed to work with your broker statements — there is no need to enter all trades and transactions manually, but it requires you to have all broker statements starting from account opening day. It may be either one broker statement or many — it doesn't matter, but what matters is that the first statement must be with zero starting assets and statements' periods mustn't overlap or have missing days in between.

For now the following brokers are supported:

* Firstrade ([details](https://github.com/KonishchevDmitry/investments/blob/master/docs/brokers.md#firstrade))
* Interactive Brokers ([details](https://github.com/KonishchevDmitry/investments/blob/master/docs/brokers.md#interactive-brokers))
* БКС ([details](https://github.com/KonishchevDmitry/investments/blob/master/docs/brokers.md#bcs))
* Сбер ([details](https://github.com/KonishchevDmitry/investments/blob/master/docs/brokers.md#sber))
* Т-Банк ([details](https://github.com/KonishchevDmitry/investments/blob/master/docs/brokers.md#tbank))

Investments keeps some data in local database located at `~/.investments/db.sqlite` and supports a number of commands which can be grouped as:

* Analyse commands ([analyse](#analyse), [cash-flow](docs/taxes.md#cash-flow), [metrics](#metrics),
  [simulate-sell](#simulate-sell), [tax-statement](docs/taxes.md#tax-statement)) that read your broker statements and produce some results. These commands use the database only for quotes caching.
* `sync` command that reads your broker statements and stores your current positions to the local database.
* Portfolio rebalancing commands ([show, rebalance, cash, buy, sell](docs/rebalancing.md)) that work only with local database.

<a name="analyse"></a>

### Performance analysis

`investments analyse` command calculates average rate of return from cash investments by comparing portfolio performance to performance of a bank deposit in USD and RUB currency with exactly the same investments and monthly capitalization. Considers taxes, commissions, dividends, tax deductions and optionally inflation when calculates portfolio performance.

<img src="/docs/images/analyse-command.png?raw=true" width="80%" height="80%" alt="investments analyse" title="investments analyse">

### Backtesting

`investments backtest` command backtests all your cash flows against configured benchmarks which allows you to compare your portfolio performace with these benchmarks.

The command also allows to backfill backtesting results (historical data) to [VictoriaMetrics](https://victoriametrics.com/) for further analysis.

<img src="/docs/images/backtest-command.png?raw=true" width="50%" height="50%" alt="investments backtest" title="investments backtest">

### Portfolio rebalancing

See [instructions for portfolio rebalancing](docs/rebalancing.md).

![investments rebalance](/docs/images/rebalance-command.png?raw=true "investments rebalance")

### Tax statement generation

**Russia:** See [instructions for tax statement generation and recommendations for interacting with Russian Federal Tax Service](docs/taxes.md).

![investments tax-statement](/docs/images/tax-statement-command.png?raw=true "investments tax-statement")

**Germany:** See [instructions for German tax statement generation](docs/germany-taxes.md). Generate a CSV statement or a printable A4 HTML report (the extension selects the format) with:

```bash
investments tax-statement ib 2024 german-tax-2024.csv
investments tax-statement ib 2024 german-tax-2024.html
```

**Spain:** See [instructions for Spanish tax statement generation](docs/spain-taxes.md). Generate a CSV report or a printable A4 report with:

```bash
investments tax-statement ib 2026 spanish-tax-2026.csv
investments tax-statement ib 2026 spanish-tax-2026.html
```

<a name="simulate-sell"></a>

### Sell simulation

`investments simulate-sell` command simulates closing of the specified positions by current market price and allows you to estimate your profits, taxes and tax exemption applicability.

![investments simulate-sell](/docs/images/simulate-sell-command.png?raw=true "investments simulate-sell")

<a name="metrics"></a>

### Prometheus metrics

`investments metrics` command allows you to export analysis results in [Prometheus](https://prometheus.io/) format to be collected by [Node exporter's Textfile Collector](https://github.com/prometheus/node_exporter#textfile-collector).

Here is an example of [Grafana](https://grafana.com/) dashboard which displays aggregated statistics and investment results for multiple portfolios opened in different brokers:

[![Investments Grafana dashboard](https://user-images.githubusercontent.com/217795/105888583-320e1080-601e-11eb-8a47-97774479e0f7.gif)](https://youtu.be/fMUxBDY3AUg)

## Non-exchange-traded unit investment funds

There is a support for non-exchange-traded unit investment funds. The program doesn't support specific fund providers along with their statements, but allows to manually specify all operations in the configuration file.

## Deposits

You can also view opened bank deposits all in one place and get notified about upcoming deposit closures. Register your opened deposits in the configuration file and then execute:

```text
$ investments deposits

                            Open deposits

 Open date   Close date    Name     Amount   Interest  Current amount
 19.06.2019  19.03.2020  Тинькофф  465,000₽         7     473,343.49₽
 21.06.2019  21.06.2020  Тинькофф  200,000₽       7.5     203,763.08₽
                                   665,000₽               677,106.57₽
```

This command has a cron mode (`investments deposits --cron`) which you can use in combination with `notify_deposit_closing_days` configuration option. For example, if you create a cron job and configure it to send the command output to your email, then on 11.06.2020 having `notify_deposit_closing_days: 10` you get an email with the following contents:

```text
The following deposits are about to close:
* 21.06.2020 Тинькофф: 200,000₽ -> 215,570.51₽

The following deposits are closed:
* 19.03.2020 Тинькофф: 465,000₽ -> 490,013.27₽
```

## Unsupported features

The program is focused on passive investing use cases and supports only those cases which I saw in my broker statements or statements sent to me by other people, which I assured to be handled properly and wrote regression tests for. For example, the following aren't supported yet:

* [Bonds](https://github.com/KonishchevDmitry/investments/issues/43)
* [Margin trading](https://github.com/KonishchevDmitry/investments/issues/8)
* [Futures and options](https://github.com/KonishchevDmitry/investments/issues/48)

## Denial of responsibility

Any automation is imperfect and the author is a software developer, not a tax lawyer, so always be critical to all program's calculation results.

The project is developed as a pet project, mainly for my personal use. The code is written in a way that if it finds something unusual in broker statement it returns an error and doesn't try to pass through the error to avoid the case when it will get you to misleading results, so there may be many cases that it's not able to handle yet and I can't guarantee that I'll find a free time to support your specific case.

## Contacts

[Issues](https://github.com/KonishchevDmitry/investments/issues) and
[Discussions](https://github.com/KonishchevDmitry/investments/discussions) are the preferred way for requests and questions. Please use [email](mailto:konishchev@gmail.com) only for privacy reasons.
