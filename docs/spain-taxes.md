# Spanish Tax Statement Generation

## Overview

Investments computes Spanish IRPF **savings-income** tax (base del ahorro) on a foreign broker
statement and writes a CSV working paper you transcribe into your return. Two regimes are supported
and they are not interchangeable:

| Regime | Law | Return |
|---|---|---|
| `gipuzkoa` | Norma Foral 3/2014, as amended by Norma Foral 1/2025 | Modelo 109, filed through Zergabidea |
| `comun` | Ley 35/2006 (LIRPF) | Modelo 100, filed through the AEAT |

They differ in the savings scale, in whether acquisition costs are actualized, in whether the two
savings-base groups may offset each other, and in whether custody fees are deductible. Every one of
those changes the tax due, so the regime has no default: omit it and the tool refuses to run.

Supported tax years: **2024, 2025, 2026**. A year the tool ships no statutory scale for is an error,
never an extrapolation.

Only the savings base is computed. Employment income, the general base, wealth tax (Impuesto sobre el
Patrimonio), partial-year residency, and the Beckham regime are all out of scope.

## Configuration

```yaml
taxes:
  jurisdiction: spain
  spain:
    regime: gipuzkoa            # or: comun

    # Saldos negativos pendientes de compensación brought in from prior returns, keyed by the year
    # each arose in. A balance may be offset only in the four following years, so the origin year
    # is part of the data: a 2022 balance is usable in a 2026 return and expired in 2027.
    loss_carryforward:
      gyp: {2024: '1200.50'}    # ganancias y pérdidas patrimoniales
      rcm: {2025: '80.00'}      # rendimientos del capital mobiliario

    # Losses the valores-homogéneos rule deferred in an earlier return, and the shares still
    # blocking them. The tool prints this block itself; copy it forward each year.
    deferred_losses:
      - {symbol: VUSA, isin: IE00B3XXRP09, loss: '420.00', blocked_quantity: 15,
         acquisition_date: 2025-12-20, sale_date: 2025-12-10}

    # Optional: override or extend the shipped Decreto Foral actualization tables, keyed by
    # disposal year then acquisition year. Lets a newly published year be used before a release.
    coefficients:
      2027: {2020: '1.11'}
```

Amounts are quoted so YAML hands the tool an exact decimal string rather than a float. Dates accept
ISO `YYYY-MM-DD` as well as the `YYYY.MM.DD` / `DD.MM.YYYY` forms the rest of the config takes; the
tool prints ISO, so its own output pastes straight back in.

Your portfolio configuration is unchanged from any other jurisdiction.

## Usage

```bash
investments tax-statement <portfolio> <year> <output.csv>
```

For example:

```bash
investments tax-statement ib 2026 spanish-tax-2026.csv
```

The command reads every statement in the portfolio, replays the whole trade history through FIFO and
the valores-homogéneos rule, computes the year's savings base, and writes the CSV plus a console
summary. The `--output` argument is optional: without it you get the summary alone.

Currency conversion uses **ECB reference rates**, selected automatically from the jurisdiction.

## Savings scales

Tax is computed once on the year's whole base after compensation, never summed from per-transaction
figures: the scale is progressive, so a sum of separately-taxed entries is not the tax on their
total.

### Gipuzkoa, 2026 onwards

Norma Foral 3/2014 art. 76.1, as replaced by NF 1/2025 art. 13.Tres with effect from 1 January 2026
(BOG nº 90, 15 May 2025):

| Base up to | Marginal rate |
|---|---|
| 7,500 | 19% |
| 15,000 | 20% |
| 30,000 | 22% |
| 50,000 | 24% |
| 90,000 | 25.5% |
| 120,000 | 26% |
| 240,000 | 26.5% |
| 300,000 | 27% |
| above | 28% |

### Gipuzkoa, 2024 and 2025

The pre-reform scale, identical in both years: 20% to 2,500 · 21% to 10,000 · 22% to 15,000 · 23% to
30,000 · 25% above.

### Territorio Común

| Base up to | 2024 | 2025 | 2026 |
|---|---|---|---|
| 6,000 | 19% | 19% | 19% |
| 50,000 | 21% | 21% | 21% |
| 200,000 | 23% | 23% | 23% |
| 300,000 | 27% | 27% | 27% |
| above | **28%** | 30% | 30% |

The top bracket was created at 28% by Ley 31/2022 (PGE 2023) and rose to 30% from 2025 under Ley
7/2024 df 7ª. There is no PGE 2026 — the 2025 budget was prorogued — and arts. 66 / 76 were not
touched, so 2026 repeats the 2025 scale.

## FIFO and actualization coefficients

Lots are consumed in acquisition order (NF 3/2014 art. 47.2 / LIRPF art. 37.2, "los adquiridos en
primer lugar"), and buy-side commissions are already inside each lot's cost.

Under **Gipuzkoa** each lot's acquisition cost is then multiplied by an actualization coefficient
(art. 45.2), published annually by Decreto Foral and keyed to that lot's acquisition year. The
coefficient is a **per-lot** figure: a single sale that consumes lots of several vintages applies a
different one to each.

Under **Territorio Común** the coefficient is always 1. Ley 26/2014 art. 1.21 deleted LIRPF art. 35.2
with effect from 2015, and even before then it applied only to real estate, never to securities.

Worked example — buy 100 AAPL at $100 on 2021-03-10, buy 100 more at $200 on 2024-06-10, sell 100 at
$250 on 2026-04-15 and 100 at $300 on 2026-09-15, no commissions, EUR/USD 0.9:

| | Sale 1 (2026-04-15) | Sale 2 (2026-09-15) |
|---|---|---|
| Proceeds | €22,500 | €27,000 |
| FIFO lot consumed | 2021, cost €9,000 | 2024, cost €18,000 |
| Coefficient (Gipuzkoa 2026) | 1.212 | 1.050 |
| Actualized cost | €10,908 | €18,900 |
| **Gipuzkoa gain** | **€11,592** | **€8,100** |
| **Común gain** | **€13,500** | **€9,000** |

Gipuzkoa ganancias €19,692 against Común's €22,500 — a difference of exactly
`9,000 × 0.212 + 18,000 × 0.050 = €2,808`, the coefficient effect and nothing else. On the Gipuzkoa
figure the 2026 scale gives a cuota of €3,957.24.

Two traps worth knowing. The table is **not** monotonic: a 1995 acquisition actualizes at 2.156 while
"1994 y anteriores" is 2.030. And an asset acquired **exactly on 31 December 1994** takes the 1995
coefficient, not the 1994-and-earlier one.

Actualization also **enlarges a loss** — art. 45.2 actualizes the acquisition value unconditionally,
with no clause restricting it to gains. A €18,000 cost from 2025 sold for €9,000 in 2026 carries
€9,360 forward under Gipuzkoa, against €9,000 under Común.

## The valores-homogéneos rule (wash sales)

A loss on listed securities is **not deductible** when homogeneous securities were acquired within
two months before or after the sale (NF 3/2014 art. 43.g / LIRPF art. 33.5.f). The loss is deferred,
not destroyed: it becomes integrable again as the blocking securities leave the estate.

How the tool applies it:

1. Every disposal in the statement is replayed in date order, across all years — a deferral created
   in one year is released in another.
2. Homogeneous means **same ISIN** (falling back to the ticker when the statement carries none).
3. The window is two **calendar** months either side, both ends inclusive, so a 31 March sale reaches
   back to 31 January and forward to 31 May.
4. Each acquired share blocks at most one sold share. The deferral is the loss scaled by the matched
   fraction of the disposal.
5. A repurchase *before* the sale blocks only shares the sale did not itself consume — those are
   gone.
6. Disposing of the blocking shares releases the deferral pro rata, dated to that disposal and
   labelled with the sale it came from.

Worked example — sell 100 shares on 2026-03-10 at a €900 loss, buy 40 back on 2026-04-20:

- 40 of the 100 sold shares are matched → `900 × 40/100 = €360` deferred, €540 deductible now.
- Selling 25 of those 40 on 2026-11-15 releases `360 × 25/40 = €225`.
- The remaining 15 shares carry €135 forward, printed as next year's `deferred_losses`.

### Carrying deferrals forward

At the end of a run the tool prints a paste-ready block:

```yaml
    deferred_losses:
      - {symbol: AAPL, isin: US0378331005, loss: '135.00', blocked_quantity: 15,
         acquisition_date: 2026-04-20, sale_date: 2026-03-10}
```

Put it in next year's config. `acquisition_date` is what matches the deferral to the sale that
releases it, so it is required.

### Statement-window caveat

A repurchase after the statement's last date cannot be seen. A loss sold in, say, December has a
window running into February, and the tool flags it:

```text
# WARNING: valores-homogéneos window still open when the statement ends.
WASH_SALE_WINDOW_OPEN,AAPL sold 2026-12-15 — window open until 2027-02-15,900.00
```

The loss is deducted in **full** in that case, which may overstate it. Export a statement that
extends at least two months past year end, or re-run when one exists.

### Out of scope

- **Unlisted securities** (art. 43.h / art. 33.5.g) use a **one-year** window, not two months. Not
  implemented.
- **Fungible crypto** (art. 43.i). Not implemented.
- Homogeneity beyond a single ISIN — the statutory definition reaches different issues of the same
  issuer with the same rights, which a broker statement cannot express.
- A deferral blocked by shares that later go through a stock split: the tool treats a corporate
  action as re-expressing shares rather than acquiring or disposing of them, so the blocked lot is
  not carried across the conversion. Check by hand if that happens.

## Loss compensation and carryforward

The savings base has two groups:

- **RCM** — rendimientos del capital mobiliario: dividends and interest, less deductible expenses.
- **GyP** — ganancias y pérdidas patrimoniales: results from transfers, including foreign-currency
  conversions.

Compensation runs in this order:

1. Each group's prior-year pending balances reduce its own positive result, **oldest vintage first**.
   Art. 49.2 requires absorbing the maximum each year and forbids stretching the window by rolling an
   old balance into a later year's losses.
2. **Territorio Común only**: a group's own negative result may then reduce the other group's
   positive, capped at **25%** of it (LIRPF art. 49.1). Gipuzkoa integrates the groups
   "exclusivamente entre sí" and skips this entirely.
3. Whatever negative remains becomes a pending balance labelled with the filing year, so its own
   four-year window starts now.

A balance whose fourth year has passed is dropped with a warning rather than carried — next year's
config must not claim an offset the tax office will refuse. A balance older than the window in the
config is a hard error, not a silent skip.

Example, RCM −2,000 against GyP +6,000:

| | Cross-offset | GyP taxable | Carried forward |
|---|---|---|---|
| Territorio Común | min(2,000, 25% × 6,000) = 1,500 | 4,500 | RCM 500 |
| Gipuzkoa | none | 6,000 | RCM 2,000 |

## Foreign tax credit

The deducción por doble imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80) is the lesser
of the tax actually paid abroad and the **savings average rate** applied to the foreign income. The
tool adds a third limb: withholding above the applicable treaty rate is not creditable in Spain at
all and must be reclaimed from the source state.

US dividend example — gross €1,000, €300 withheld (30%), savings base €10,000 under Gipuzkoa 2026:

- Treaty limb: `1,000 × 15% = €150`
- Rate limb: average rate `1,925 / 10,000 = 0.1925`, so `1,000 × 0.1925 = €192.50`
- **Credit: €150.** The other €150 is over-withholding; reclaim it from the IRS with a W-8BEN and
  Form 1040-NR, not through the Spanish return.

The credit is a year-level figure — its cap depends on the average rate, which only exists once the
whole base is known. The per-row `treaty_capped_credit_eur` column is informational.

## Custody and administration fees

This is the sharpest split between the regimes.

- **Territorio Común**: "gastos de administración y depósito de valores negociables" are deductible
  from RCM (LIRPF art. 26.1.a). The fee for *gestión discrecional e individualizada de carteras* is
  excluded by name.
- **Gipuzkoa**: nothing is deductible. There is no equivalent of art. 26.1.a; NF 3/2014 art. 39 is a
  closed list that allows deductions only for asistencia técnica, arrendamiento de bienes muebles,
  negocios o minas, and subarrendamientos. Confirmed by the Diputación Foral's own Renta manual,
  ch. 4 §4.5.

A broker statement carries only a free-text description, so the tool matches on keywords (custody,
safekeeping, administration, custodia, administración). An unrecognised fee is **reported but not
deducted** — that overstates tax rather than understating it. Trading commissions are not affected
either way: they are already inside the FIFO cost basis.

## Foreign-currency gains

A currency conversion transfers a patrimonial element, so its result joins the **ganancias** group.
Balances are tracked with a signed-inventory FIFO over the cash ledger.

Results realized on a **borrowed** (margin) balance are excluded from the base and reported for
manual review instead: repaying a currency loan is not clearly a transfer of a patrimonial element,
and neither the foral nor the state text settles it.

## Other income

- **Stock grants / RSUs** are employment income and belong to the *general* base, which this tool
  does not compute. Vested shares are costed at their vest-date fair market value when later sold.
- **Short positions** open at the end of the statement get no automatic treatment; they are listed
  for manual review.
- **Corporate actions** are reported for information; their tax impact is not computed.

## Modelo 109 / Modelo 100 mapping

The CSV ends with a box mapping, and every number in it carries a `# WARNING`.

**Gipuzkoa publishes no static numbered form** — the return is generated by Zergabidea — and the only
official box map reachable is explicitly dated **AÑO 2019**: RCM casillas 06+16, deductible expenses
17, retenciones 07+22, ganancias patrimoniales 28, base imponible del ahorro 33, cuota líquida 64.
The 2026 reform added a cuota-íntegra column to art. 76.1 and a crypto section to Anexo 3, either of
which plausibly renumbered boxes. Verify every casilla against your filing year's own form.

The casilla for the deducción por doble imposición internacional is not published anywhere reachable,
for any year. It is emitted as `MODELO_109_CASILLA_UNKNOWN`.

For **Territorio Común** the rows carry labels but no casilla numbers: Modelo 100's box layout was
not verified for this tool.

## CSV output

See [`specs/002-spain-tax/contracts/csv-output.md`](../specs/002-spain-tax/contracts/csv-output.md)
for the full contract. In short: a 19-column transaction block where each capital gain is followed by
its per-FIFO-lot `Lot` rows (so the actualization multiplication reads line by line), then a
3-column `summary_key,label,value_eur` block holding the group totals, the compensation, the
carry-forward and deferral blocks, and the Modelo mapping. Comment lines start with `#`.

## Sell simulation

`investments simulate-sell` prices each hypothetical disposal at what it **adds to the tax year**,
not at a flat rate — because a Spanish disposal has no standalone price. The savings scale is
progressive, the groups are compensated against prior-year balances first, each lot is actualized by
its own acquisition year, and a loss on a holding just repurchased is deferred rather than deducted.
The tool prints the year's groups, base and cuota with and without the sale, so you can see where the
figure comes from.

With several positions the per-row tax is **marginal and order-dependent**: each row is what that sale
adds on top of the ones above it. Only the total is order-independent.

## Important notes

1. **Currency conversion** uses ECB reference rates on the transaction date.
2. **FIFO is per depot.** A multi-account Flex Query is rejected rather than processed — pooling
   accounts would match sells against the wrong depot's lots.
3. **Separate ownership forms run separate FIFO queues.** The Diputación Foral's own guidance treats
   solely-owned and jointly-held holdings as distinct; the tool does not model joint ownership.
4. **Modelo 720** (informational declaration of assets abroad) is a separate obligation. This tool
   never computes it — check whether you are required to file it.
5. **No Vorabpauschale equivalent.** Accumulating funds simply defer taxation until disposal.
6. **Fund traspaso deferral does not apply at a foreign broker.** The rollover regime for Spanish
   collective-investment institutions requires the transfer to run through the Spanish system.
7. **Self-declaration.** The CSV is a working paper. You still file through Zergabidea or the AEAT.
8. **Tax advisor.** Wash-sale timing, joint ownership, and cross-border residency questions should be
   reviewed with a qualified adviser.

## Troubleshooting

### "No Spanish savings-base scale is shipped for … tax year"

The tool ships statutory scales for 2024–2026 only and refuses to extrapolate. Filing an earlier or
later year needs a release that adds that year's law.

### "sales in \<year\> were not tested for the valores-homogéneos rule"

The statement contains disposals in a year with no shipped actualization table, so their result could
not be priced and their losses were never tested for deferral. Add that year's table under
`taxes.spain.coefficients.<year>`, or carry the deferral in `taxes.spain.deferred_losses` from that
year's return.

### "… has expired: a negative savings-base balance may be offset only in the four following years"

A `loss_carryforward` entry is older than the window. Remove it — it can no longer be used.

### Missing exchange rates

A conversion failure names the date and currency. It usually means the ECB published no reference
rate for that date; check that the date is a TARGET business day.
