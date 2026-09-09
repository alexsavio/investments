# Spanish Tax Statement Generation

## Overview

Investments computes Spanish IRPF **savings-income** tax (base del ahorro) on a foreign broker
statement and writes a CSV working paper you transcribe into your return. Three regimes are
supported and they are not interchangeable:

| Regime | Law | Return |
|---|---|---|
| `gipuzkoa` | Norma Foral 3/2014, as amended by Norma Foral 1/2025 | Modelo 109, filed through Zergabidea |
| `comun` | Ley 35/2006 (LIRPF) | Modelo 100, filed through the AEAT |
| `navarra` | Decreto Foral Legislativo 4/2008 (TRLFIRPF) | Modelo F-93, filed through Hacienda Foral de Navarra |

They differ in the savings scale, in whether acquisition costs are actualized, in how the two
savings-base groups may offset each other, in whether custody fees are deductible and by how much,
and in which gains are exempt. Every one of those changes the tax due, so the regime has no default:
omit it and the tool refuses to run.

### What separates the three

| | Gipuzkoa | Territorio Común | Navarra |
|---|---|---|---|
| Savings scale | NF 3/2014 art. 76.1 — reformed for 2026 | LIRPF arts. 66/76 — top bracket 30% from 2025 | TRLFIRPF art. 60 — one table for 2024–2026 |
| Actualization of acquisition cost | Decreto Foral coefficients, per acquisition year | none (coefficient 1) | none (coefficient 1) — art. 41 never had it |
| Cross-group offset | never: the groups integrate "exclusivamente entre sí" | 25%, AEAT Manual cap. 12 order | 25%, but art. 54.2's own order (own-group carryforwards first) |
| Custody and administration fees | not deductible at all | deductible, no ceiling | deductible, capped at 3% of non-exempt gross securities income (art. 32.1.a) |
| Dividend exemption | first €1,500 a year (art. 9.24) | none since 2015 | none since 2015 (LF 29/2014) |
| Small-disposals exemption | none | none | year's transmissions ≤ €3,000 → gain exempt up to half of them (art. 39.5.d) |
| Wash-sale window | 2 months, listed | 2 months, listed | 2 months, listed (art. 39.6.f) |
| Loss carry-forward | 4 years | 4 years | 4 years |

On the same trades those differences do not cancel out. The `fifo` test fixture — two AAPL sales
totalling €49,500 of proceeds — produces a cuota íntegra del ahorro of **€3,957.24** under Gipuzkoa,
**€4,605.00** under Común and **€5,230.00** under Navarra. Navarra shares Común's base and neither
regime's tax.

Supported tax years: **2024, 2025, 2026**. A year the tool ships no statutory scale for is an error,
never an extrapolation.

Only the savings base is computed. Employment income, the general base, wealth tax (Impuesto sobre el
Patrimonio), partial-year residency, and the Beckham regime are all out of scope.

## Configuration

```yaml
taxes:
  jurisdiction: spain
  spain:
    regime: gipuzkoa            # or: comun, navarra

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
    # Gipuzkoa only: Común and Navarra have no actualization, and under `navarra` the block is
    # rejected outright rather than silently ignored.
    coefficients:
      2027: {2020: '1.11'}
```

Amounts are quoted so YAML hands the tool an exact decimal string rather than a float. Dates accept
ISO `YYYY-MM-DD` as well as the `YYYY.MM.DD` / `DD.MM.YYYY` forms the rest of the config takes; the
tool prints ISO, so its own output pastes straight back in.

Your portfolio configuration is unchanged from any other jurisdiction.

## Usage

```bash
investments tax-statement <portfolio> <year> <output.csv|output.html>
```

For example:

```bash
investments tax-statement ib 2026 spanish-tax-2026.csv
investments tax-statement ib 2026 spanish-tax-2026.html
```

The command reads every statement in the portfolio, replays the whole trade history through FIFO and
the valores-homogéneos rule, computes the year's savings base, and writes the output file plus a
console summary. The output path is an optional positional argument: without it you get the summary
alone. The extension picks the format: `.html` is the printable report described below, anything
else the CSV.

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

### Navarra

TRLFIRPF art. 60, in the wording Ley Foral 36/2022 gave it with effect from 1 January 2023. One table
for every supported year: Ley Foral 17/2025, which carries the 2026 changes, does not touch it.

| Base liquidable hasta | Cuota íntegra | Resto base hasta | Tipo |
|---|---|---|---|
| — | — | 6,000 | 20% |
| 6,000 | 1,200 | 4,000 | 22% |
| 10,000 | 2,080 | 5,000 | 24% |
| 15,000 | 3,280 | 185,000 | 26% |
| 200,000 | 51,380 | 100,000 | 27% |
| 300,000 | 78,380 | rest | 28% |

Unlike the other two, the Navarra statute publishes its own cumulative cuota-íntegra column, so those
five figures are the law's own numbers rather than a derivation. The tool reproduces all five exactly;
above the last threshold there is no published cuota and the 28% marginal rate applies.

## FIFO and actualization coefficients

Lots are consumed in acquisition order (NF 3/2014 art. 47.2 / LIRPF art. 37.2, "los adquiridos en
primer lugar"), and buy-side commissions are already inside each lot's cost.

Under **Gipuzkoa** each lot's acquisition cost is then multiplied by an actualization coefficient
(art. 45.2), published annually by Decreto Foral and keyed to that lot's acquisition year. The
coefficient is a **per-lot** figure: a single sale that consumes lots of several vintages applies a
different one to each.

Under **Territorio Común** the coefficient is always 1. Ley 26/2014 art. 1.21 deleted LIRPF art. 35.2
with effect from 2015, and even before then it applied only to real estate, never to securities.

Under **Navarra** the coefficient is always 1 as well, for a different reason: TRLFIRPF art. 41 has
never carried an actualization rule at all. A `taxes.spain.coefficients` block under that regime is
therefore rejected with an error naming the regime — accepting and ignoring it would leave you
believing costs were actualized when they were not.

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
two months before or after the sale (NF 3/2014 art. 43.g / LIRPF art. 33.5.f / TRLFIRPF art. 39.6.f).
The loss is deferred,
not destroyed: it becomes integrable again as the blocking securities leave the estate.

How the tool applies it:

1. Every disposal in the statement is replayed in date order, across all years — a deferral created
   in one year is released in another.
2. Homogeneous means **same ISIN** (falling back to the ticker when the statement carries none). The
   same test gates the Gipuzkoa dividend exemption's anti-abuse clause, so a renamed line cannot slip
   past either rule under its other ticker.
3. The window is two **calendar** months either side, both ends inclusive, so a 31 March sale reaches
   back to 31 January and forward to 31 May. A period fixed in months runs "de fecha a fecha"
   (Código Civil art. 5.1, supletory through LGT art. 7.2), and the Tribunal Supremo treats the
   terminal ordinal as the **last day of the period**, not the first day past it — STS 552/2022
   (RC 1874/2021), STS 02-07-2020 (RC 3780/2019), STS 02-04-2008 (rec. 323/2004: published 13-02, a
   one-month period expires 13-03, a filing on 15-03 is late). Where that ordinal does not exist the
   period ends on the last day of the month (CC art. 5.1; Ley 39/2015 art. 30.4), which is why a
   31 December sale reaches forward only to 28 February.
4. Each acquired share blocks at most one sold share. The deferral is the loss scaled by the matched
   fraction of the disposal.
5. A repurchase *before* the sale blocks only shares the sale did not itself consume — those are
   gone.
6. Disposing of the blocking shares releases the deferral pro rata, dated to that disposal and
   labelled with the sale it came from.
7. A release only counts to the extent that disposal was itself **definitive**. Both statutes make
   the loss integrable "a medida que se transmitan los activos", and DGT V3282-18 reads that as
   requiring a real exit: sell the blocking shares and buy homogeneous ones back inside the window
   and the deferral does not end, it moves onto the new shares. The matched part is re-attached
   (keeping the original sale's label), the rest becomes integrable.
8. That split is measured on the **blocked** shares the disposal consumed, not on the whole
   disposal. Selling 40 blocked shares together with 60 unblocked ones and buying 40 back replaces
   every share that was blocking, so nothing left the estate for good and the whole deferral moves
   on. V3282-18 gives no allocation rule for a mixed disposal; attributing the repurchase to the
   blocked shares first is the conservative reading — it re-attaches more and integrates less, so it
   postpones a deduction rather than granting one early.

**Not modelled: the one-year window.** Both statutes carry a fourth limb for securities *not*
admitted to trading on a regulated market as defined in Directive 2014/65/UE — there the window is
one **year** either side, not two months. The tool does not implement it, and applies two months to
every instrument in the statement.

For US listings that is now the tax authority's own reading, not a guess. **DGT CV V0778-25**
(05-05-2025) answers the question for NYSE, Nasdaq and CME by name, and **V0951-25** (30-05-2025)
generalizes it to "los mercados de valores de Estados Unidos": a third-country market covered by an
**in-force Commission equivalence decision** under MiFID II art. 25(4)(a) is inside art. 33.5.f, and
stays inside it "mientras dicha decisión de equivalencia no haya sido objeto de derogación". US
venues are equivalent under Commission Implementing Decision **(EU) 2017/2320**, Australia under
**2017/2318**, Hong Kong under **2017/2319**. Following a published DGT criterion also shields the
filer from penalties (LGT art. 179.2.d).

Both foral texts clone the state wording and interpret the same EU concept, but neither has a
pronouncement of its own, so for a Gipuzkoa or Navarra filer these criteria are persuasive rather
than formally binding. **TRLFIRPF art. 39.6.f cites Directive 2014/65/UE (MiFID II) directly**, where
the state text still points at the repealed 1993 Ley del Mercado de Valores — so the equivalence
machinery the DGT reasons from is named in the Navarra article itself rather than reached through a
chain of superseded references.

Two consequences the tool acts on:

- **Switzerland's decisions lapsed on 30-06-2019** and were never renewed, and the United Kingdom,
  Canada and Japan have none. A line listed only on such a venue may fall under the one-year limb.
- Where a repurchase lands **outside the two months but inside the year**, the deferral therefore
  turns on the venue. The tool still defers nothing there — the window does not change — but it
  reports the amount at stake, using the listing exchange the statement names for the instrument:

  ```text
  # WARNING: the deduction below turns on which valores-homogéneos window the listing venue takes.
  WASH_SALE_VENUE_REVIEW,SWCH sold 2026-03-10 — listed on EBS,900.00
  ```

  Equivalent venues (US, EEA, ASX, SEHK) say nothing. A statement that names no venue for the
  instrument is reported too: unknown is not the same as settled.

**When a deferral hangs on one day.** No authority applies that arithmetic to art. 33.5.f with
concrete dates, so where an outcome actually turns on a window edge the sale row says so — a
repurchase on the terminal day itself, one day outside it, or inside the span a month-end clamp
constructs:

```text
€900.00 of the AAPL loss of 2026-03-10 turns on window-boundary arithmetic: homogeneous securities
were acquired on the window's own terminal day, … The tool puts that edge on 2026-05-10; the other
reading puts it on 2026-05-09. …
```

The figure is what moves between deferred and deductible under the other reading. A repurchase well
inside the window says nothing.

**Dual listings and ADRs.** DGT CV **V1872-25** (14-10-2025) holds that two lines of the same class
listed in different markets and currencies *are* homogeneous, which is what matching on the ISIN
does. It expressly declines to answer whether an ADR is homogeneous with the underlying ordinary
share; those carry different ISINs, so the tool treats them as different securities.

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

The block is a snapshot of what is still blocked on **31 December of the filing year**. Disposals
after that date are still replayed — that is how the two-month window closes — but only to report
releases against the following return; they never rewrite this year's carry-out.

**Never carry in a deferral whose loss-making sale the statement also contains.** The tool computes
that sale's deferral from the statement itself, so the config entry would both block the shares the
statement's own deferral needed and release separately, deducting the loss twice. It refuses to run
in that case:

```text
taxes.spain.deferred_losses entry for AAPL names a loss-making sale on 2026-03-10 that this
statement already contains and prices, so the tool computes that deferral itself. Keeping both
would deduct the loss twice — remove the config entry. …
```

The guard only fires against a sale the tool could actually **price**. A disposal in a year no
actualization table is shipped for is replayed but never priced, so it can never compute a deferral
of its own — there the carried-in entry is the only record of one and is accepted. The alternative is
to ship that year's table under `taxes.spain.coefficients.<year>` and let the replay do the work.

The entries to carry in are the ones whose loss-making sale predates the statement, or falls in a
year the tool cannot price.

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

- **Unlisted securities** (art. 43.h / art. 33.5.g / TRLFIRPF art. 39.6.g) use a **one-year** window,
  not two months. Not implemented.
- **Fungible crypto** (art. 43.i). Not implemented.
- Homogeneity beyond a single ISIN — the statutory definition reaches different issues of the same
  issuer with the same rights, which a broker statement cannot express.
- Complex splits (those producing fractional stock, which the tool models as a synthetic sell + buy)
  are still not carried across: a corporate action re-expresses shares rather than acquiring or
  disposing of them, so its trades are excluded from the matching and a blocked lot does not survive
  the conversion. Plain splits **are** handled — every quantity the rule sees is normalized to
  post-split units as of the statement's last date, so a 2-for-1 between the loss and the repurchase
  matches and releases the right fraction.
- A `deferred_losses` entry carried in from a prior return is taken in the units that return
  reported. A split between that acquisition and the current statement is not applied to it; adjust
  the `blocked_quantity` by hand if one happened.

## Loss compensation and carryforward

The savings base has two groups:

- **RCM** — rendimientos del capital mobiliario: dividends and interest, less deductible expenses.
- **GyP** — ganancias y pérdidas patrimoniales: results from transfers, including foreign-currency
  conversions.

### Gipuzkoa

The groups are integrated "exclusivamente entre sí" (Manual de Renta cap. 9) and never touch. Each
group's prior-year pending balances reduce its own positive result, **oldest vintage first** — art.
49.2's foral equivalent requires absorbing the maximum each year and forbids stretching the window by
rolling an old balance into a later year's losses. Whatever negative remains becomes a pending
balance labelled with the filing year, so its own four-year window starts now.

### Territorio Común

Two phases, per the AEAT Manual Práctico de Renta cap. 12 (LIRPF art. 49):

1. **Fase 1ª** — the year's own results meet each other first: a current-year negative in one group
   reduces the other group's current-year positive.
2. **Fase 2ª-1º** — prior-year pending balances reduce what is left of **their own** group, oldest
   vintage first.
3. **Fase 2ª-2º** — a prior-year balance its own group could not absorb crosses into the other
   group's remainder.

The **25% limit is one allowance per group**, measured on that group's *original* current-year
positive and shared by steps 1 and 3. It is not re-measured after prior-year balances are absorbed.

The manual's own worked example — current GyP +4,000, current RCM −800, prior-year GyP balance
2,800, prior-year RCM balance 500:

| Step | Effect | GyP left | Allowance left |
|---|---|---|---|
| Allowance | 25% × 4,000 | 4,000 | 1,000 |
| Fase 1ª | current RCM −800 crosses | 3,200 | 200 |
| Fase 2ª-1º | prior GyP 2,800 absorbed | 400 | 200 |
| Fase 2ª-2º | prior RCM crosses, capped at 200 | **200** | 0 |

Savings base **200**; 300 of the prior RCM balance survives with its original vintage.

The CSV reports the two prior-year steps **disjointly**, so nothing is counted twice:
`SUMMARY_RCM_LOSSES_APPLIED` carries only what Fase 2ª-1º applied inside the RCM group, and
`SUMMARY_PRIOR_CROSS_OFFSET_RCM_TO_GYP` carries only what Fase 2ª-2º crossed out of it. Their sum is
the prior-year RCM balance this year consumed — €200 in the example above, all of it crossed, so the
own-group row reads 0. The ganancias pair works the same way.

### Navarra

TRLFIRPF art. 54.2 runs the same three moves in a different order, and measures the 25% on a
different figure. Per group, independently first:

1. Sum the year's own items.
2. **Only if that result is positive**, absorb the group's own prior-year saldos, oldest vintage
   first, floored at zero. A group whose result is negative leaves its own saldos untouched — the
   statute opens the absorption branch for a positive result only.
3. A negative result then crosses into "el saldo positivo **resultante de la letra b)**", capped at
   25% of it. That figure is what the other group arrived at *after* absorbing its own
   carryforwards, not its raw positive.
4. What is left carries four years "en el mismo orden establecido en los párrafos anteriores".

Run the AEAT manual's own numbers — current GyP +4,000, current RCM −800, prior-year GyP 2,800,
prior-year RCM 500 — through all three regimes and no two agree:

| Regime | Route | Savings base |
|---|---|---|
| Gipuzkoa | GyP absorbs its own 2,800; nothing crosses | **1,200** |
| Territorio Común | cross 800, then 2,800, then 200 of the prior RCM saldo | **200** |
| Navarra | GyP absorbs its own 2,800 → 1,200; the RCM −800 then crosses at 25% × 1,200 = 300 | **900** |

With **no** prior-year balances step 2 does nothing, so Navarra and Común give the same answer. The
orders only separate once a carryforward exists.

Whether a *carried* saldo may cross at all is the one part art. 54.2 does not settle; see
[Open interpretations §11](#11-which-navarra-saldos-may-cross-the-groups--open) for what the tool
does and what it says when the point bites.

### All three regimes

A balance whose fourth year has passed is dropped with a warning rather than carried — next year's
config must not claim an offset the tax office will refuse. A balance older than the window in the
config is a hard error, not a silent skip.

Simple example with no prior-year balances, RCM −2,000 against GyP +6,000:

| | Cross-offset | GyP taxable | Carried forward |
|---|---|---|---|
| Territorio Común | min(2,000, 25% × 6,000) = 1,500 | 4,500 | RCM 500 |
| Navarra | min(2,000, 25% × 6,000) = 1,500 | 4,500 | RCM 500 |
| Gipuzkoa | none | 6,000 | RCM 2,000 |

## Foreign tax credit

The deducción por doble imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80) is the lesser
of the tax actually paid abroad and the **savings average rate** applied to the foreign income. The
tool adds a third limb: withholding above the applicable treaty rate is not creditable in Spain at
all and must be reclaimed from the source state.

The two limbs measure **different bases**:

- The treaty limb caps what the *source* state may levy, so it runs on the gross that state taxed —
  but **per payment**, and only on the slice of it Spain actually taxes. Per payment because a
  treaty caps each payment separately: pooling the year's withholding against the year's gross would
  let a dividend withheld at 0% lend its unused headroom to one withheld at 30%. Only on the taxed
  slice because income Spain exempts bears no Spanish tax, so it carries no credit — the Gipuzkoa
  €1,500 exemption is spread pro rata across the payments eligible for it and comes off their gross
  before the cap is applied.
- The rate limb caps the *Spanish* tax on that income, so it runs on the **net that actually reaches
  the base liquidable**. TEAC resolución RG 00/08643/2023 (20-10-2025, unificación de criterio,
  binding on the administration per LGT art. 239.8) settles this: the income counts "una vez
  deducidos los gastos y compensadas las rentas". The tool subtracts deductible expenses pro-rated
  by the foreign share of RCM income, then whatever compensation removed from the RCM group. A group
  a prior-year balance wiped carries no foreign income into the base, so the credit is **zero** —
  even if the year still pays tax on its ganancias. The figure is printed as
  `SUMMARY_FOREIGN_TAXABLE_INCOME`.

The average rate itself is "expresado con dos decimales" (NF 3/2014 art. 76.2 / LIRPF art. 80.2) —
two decimals as a *percentage*, four as a fraction — and that is operative, not presentation. The
AEAT Manual Práctico de Renta cap. 18 works its example from the rounded rate: `16,60% × 6.000 € =
996 €`. The tool rounds before multiplying.

Two worked examples, both on a single US dividend of gross €1,000 with €300 withheld (30%), nothing
deductible and no compensation.

**Gipuzkoa** — the dividend is below the €1,500 annual exemption, so the whole €1,000 is exempt:

- Taxed slice of the payment: €0, so the treaty limb is `min(300, 0 × 15%) = €0`
- Rate limb: `0 × average rate = €0`
- **Credit: €0.** The Spanish return never taxed this income, so there is no Spanish tax for the
  foreign tax to be credited against. The whole €300 is reclaimed from the IRS — file a W-8BEN so
  only 15% is withheld next time, and claim the €150 already over-withheld on Form 1040-NR.

**Territorio Común** — no exemption, and a savings base of €10,000:

- Taxed slice of the payment: €1,000, so the treaty limb is `min(300, 1,000 × 15%) = €150`
- Rate limb: the 2026 state scale gives `6,000 × 19% + 4,000 × 21% = €1,980`, an average rate of
  `1,980 / 10,000 = 19.80%`, so `1,000 × 0.1980 = €198`
- **Credit: €150.** The treaty limb binds; the other €150 is over-withholding and is reclaimed from
  the IRS, not through the Spanish return.

The credit is a year-level figure — its rate limb depends on the average rate, which only exists once
the whole base is known. The per-row `treaty_capped_credit_eur` column is a different figure: it is
measured on the **full** gross, because that is what a reclaim from the source state is measured
against, so its column does not sum to the credit's first limb wherever an exemption applies.

### The treaty rate is hard-coded at 15% for dividends

That is the portfolio-dividend rate in Spain's treaties with the **US, Germany, Ireland, the
Netherlands, France and Switzerland**. Three caveats, none of which the tool detects:

| Case | Rate | Consequence |
|---|---|---|
| UK portfolio dividends | **10%** | The tool credits up to 15%; reduce it by hand. 15% applies only to REIT PIDs |
| US REIT, holder above a 10% stake | **no treaty benefit** | The full 30% stands and none of the excess is creditable in Spain |
| Interest | 0-10%, never 15% | Moot in practice: IB's interest accruals carry no withholding field, so the tool never credits interest withholding at all |

Neither NF 3/2014 art. 91 nor LIRPF art. 80 mentions a treaty cap — the limit comes from the treaty
itself. Check yours if the source state is not one of the six above.

## The Gipuzkoa €1,500 dividend exemption

Gipuzkoa exempts the first **€1,500** of dividends and participaciones en beneficios each year
(NF 3/2014 **art. 9.24**, referring to art. 34.1.a/b). There is no residence restriction on the
payer, so foreign dividends qualify. Verified in force for **2024, 2025 and 2026**: the Diputación
Foral's own Modelo 109 pages list it verbatim as exempt item 24 for ejercicios 2023, 2024 and 2025,
and neither NF 1/2025 (the 2026 reform) nor NF 2/2025 touches art. 9 número 24 — both only add new
números (38, 39, 40) and amend 1, 5, 6 and 7.

**Territorio Común has no equivalent.** Ley 26/2014 repealed LIRPF art. 7.y with effect from 2015.

Consequences the tool applies:

- The exempt slice never enters the savings base, so it also carries **no double-taxation credit**:
  the credit's rate limb runs on the income that actually reaches the base liquidable, and exempt
  income does not. A Gipuzkoa filer whose only dividends total less than €1,500 pays no Spanish tax
  on them and credits none of the foreign withholding — reclaim the excess from the source state.
- The **anti-abuse clause** is applied: a dividend on securities acquired within the two months
  before the payment date is excluded when homogeneous securities are transferred within the two
  months after it. The tool applies this per instrument rather than per share, because a broker
  statement cannot say which shares a payment came from; that excludes more than the statute
  strictly requires, which overstates tax rather than understating it.
- **Distributions from instituciones de inversión colectiva** (funds, ETFs, SICAVs) and interest on
  cooperative contributions do **not** qualify, and an IB statement does not distinguish them from
  company dividends. The tool exempts them anyway and prints a warning naming every payer it
  exempted — check each one and reduce the exemption by hand if any is a fund.

## Custody and administration fees

This is the sharpest split between the regimes.

- **Territorio Común**: "gastos de administración y depósito de valores negociables" are deductible
  from RCM (LIRPF art. 26.1.a). The fee for *gestión discrecional e individualizada de carteras* is
  excluded by name.
- **Gipuzkoa**: nothing is deductible. There is no equivalent of art. 26.1.a; NF 3/2014 art. 39 is a
  closed list that allows deductions only for asistencia técnica, arrendamiento de bienes muebles,
  negocios o minas, and subarrendamientos. Confirmed by the Diputación Foral's own Renta manual,
  ch. 4 §4.5.
- **Navarra**: the same fees are deductible as under Común (TRLFIRPF art. 32.1.a, which also
  excludes discretionary portfolio management by name), but **capped at 3% of the non-exempt gross
  income from those securities**. See below.

A broker statement carries only a free-text description, so the tool matches on it. Under Común and
Navarra each type is treated as the DGT classifies it:

| Fee type | Treatment | Authority |
|---|---|---|
| Custody, safekeeping, depósito, administración de valores | **Deducted** from RCM | LIRPF art. 26.1.a; DGT V2117-19 |
| Dividend collection, coupon handling, corporate-event handling | **Deducted** (medium confidence: the DGT reads them into the depósito service, the article does not name them) | DGT V2117-19 |
| Trading commissions, exchange / SEC / FINRA / canon / stamp / FTT pass-throughs | Not an art. 26 expense — they adjust the acquisition and transmission values, which the per-trade figures already do | DGT V2629-13 |
| Management, advisory, performance / success fees | **Not deductible**, excluded by name | LIRPF art. 26.1.a; DGT V1047-16 |
| Market data, research, quotes | **Not deductible**: not part of the deposit function | Consulta 03-04-1998; AEAT Manual cap. 5 |
| Wire / withdrawal / SEPA, current-account fees | **Not deductible**: moving cash is not a cost of holding securities | Consulta 03-04-1998 |
| Inactivity, minimum-activity, maintenance, connectivity | **Not deducted, and flagged**: no doctrine either way | — |
| Standalone currency-conversion fees | **Not deducted, and flagged** | — |
| Securities transfer-out (traspaso) fees | **Not deducted, and flagged** | — |

An unrecognised description is **reported but not deducted** — that overstates tax rather than
understating it. The three flagged types get an explicit warning on the console, in the log and in
the fee row's `notes`:

```text
€27.00 of inactivity, minimum-activity or maintenance fee on 2026-09-15 is NOT deducted from the
savings base: no DGT doctrine settles whether it is a gasto de administración y depósito under
LIRPF art. 26.1.a. …
```

**A foreign broker charging the fee does not change the answer.** No consulta is on point, but the
statutory wording is not territorially limited and AEAT practice accepts the deduction; treat that as
medium confidence.

None of this applies under Gipuzkoa, where art. 39 allows nothing whatever the type is: every fee row
is informational there, and no type can raise an open question.

### The Navarra 3% ceiling

TRLFIRPF art. 32.1.a allows the fees "con el límite del 3 por 100 de los ingresos íntegros, que no
hayan resultado exentos, procedentes de dichos valores". The tool measures that ceiling on **dividend
income**: interest credited on a broker cash balance is a rendimiento from the cesión a terceros de
capitales propios (art. 29), not income from a valor negociable.

What that does to a year of €900 of dividends, €90 of broker interest and a €45 custody fee:

| | Común | Navarra |
|---|---|---|
| Ceiling | none | 3% × 900 = **27.00** |
| Deducted | 45.00 | **27.00** |
| Disallowed by the ceiling | — | **18.00** |
| Rendimiento neto | 945.00 | **963.00** |

Two consequences worth knowing:

- A year with fees but **no dividends** has a ceiling of zero, so nothing is deductible at all.
- Because the ceiling is a fraction of dividend income, fees alone can never drive the Navarra RCM
  result negative.

The CSV reports the ceiling and what it disallowed on their own rows (`SUMMARY_RCM_FEE_CAP`,
`SUMMARY_RCM_FEES_OVER_CAP`), kept separate from `SUMMARY_INFORMATIONAL_FEES`: those fees never
qualified, these did and were capped. A binding ceiling also prints a warning. See
[Open interpretations §12](#12-what-the-navarra-3-fee-ceiling-is-measured-on--open-by-data-limit).

### Interest paid on a margin loan

IB reports margin interest as a **negative** "Broker Interest Paid" accrual in the same ledger as the
credit interest, so a naive sum would net it off your RCM income. None of the three regimes allows
that: LIRPF art. 26.1.a and TRLFIRPF art. 32.1.a reach only administration and custody of negotiable
securities, and NF 3/2014 art. 39 is narrower still. The tool reports each paid-interest row without a savings group, sums them into
`SUMMARY_RCM_INTEREST_PAID`, and leaves the RCM result untouched.

## The Navarra €3,000 small-disposals exemption

TRLFIRPF **art. 39.5.d** exempts gains arising on onerous transmissions when two conditions hold
together, both measured over the whole calendar year:

1. the global amount of those transmissions does not exceed **€3,000**;
2. the taxable increment does not exceed **50%** of that global amount — and where it does, "únicamente
   se someterá a gravamen el citado exceso".

Neither the state text nor NF 3/2014 has anything like it.

Worked, on a year that buys 10 shares for €900 and sells 6 for €1,620 and 4 for €450:

| | Value |
|---|---|
| Global transmission amount | 2,070.00 |
| Taxable increment | 1,170.00 |
| Half the global amount | 1,035.00 |
| Exempt | **1,035.00** |
| Taxed | **135.00** |

Both boundaries are inclusive: exactly €3,000 of transmissions still qualifies, and an increment of
exactly half the global amount is wholly exempt. One euro over €3,000 and the whole relief is gone —
it is a gate, not a taper.

Both conditions are measured **over the year as a whole**, not disposal by disposal. In the year
above the first sale's gain (€1,080) is more than half its own proceeds while the second's (€90) is
less than half of its own; one year-wide denominator lets the second sale's unused headroom shelter
part of the first sale's excess, and a per-disposal denominator would exempt €900 instead of €1,035.
See [Open interpretations §13](#13-the-navarra-3000-exemptions-global-amount--open).

Proceeds are counted for **every** disposal, gain- or loss-making, because condition 1 measures the
transmissions rather than their results; only positive integrable results feed the increment, because
a loss is a *disminución*, not an *incremento*. A wash-sale-deferred loss therefore leaves the
proceeds alone and stays out of the increment.

**The relief can never eat a loss.** A year with a €900 gain on €1,800 of proceeds and a €450 loss on
€900 has a global amount of €2,700 and an increment of €900 — not the €450 net. The exemption takes
the whole €900 and the €450 loss survives in full, so the ganancias group closes at −450 and carries
that forward. The F-93 says the same thing structurally: Anexo 1 gives each transmission separate
*Incremento* (656) and *Disminución* (657) cells, and the *Incremento exento. Otros supuestos* cell
(1658) sits under the incremento only.

**Foreign-currency conversions withhold the relief.** A conversion is a transmission too (art.
54.1.b), but the tool records only its result, never the amount converted, so a year that has one
cannot have its global amount measured. The exemption is then not applied — which overstates the tax
rather than granting a relief the year may not be entitled to. The reason is printed where the
relief was actually withheld, and three boundaries keep it quiet elsewhere: a year with no
transmission *incremento* (nothing to relieve), conversions on a **borrowed** balance (they never
reach the ganancias group), and securities proceeds already above €3,000 (the missing conversions
can only add to a total that has failed). See
[Open interpretations §13](#13-the-navarra-3000-exemptions-global-amount--open).

**A conversion gain is never relieved in its own right.** Whatever the year looks like, the tool
feeds only securities results into the exemption's increment; a conversion result is taxed in full
and counts towards neither figure the article measures. That is a scope limit of this tool, not a
reading of art. 39.5.d, and the CSV's own `SUMMARY_GYP_FX` row says so under Navarra on every run.

## Foreign-currency gains

A currency conversion transfers a patrimonial element, so its result joins the **ganancias** group in
all three regimes; under Navarra it is TRLFIRPF art. 54.1.b that puts it there, and art. 78.7 that
imputes it to the year of the conversion. Balances are tracked with a signed-inventory FIFO over the
cash ledger.

Results realized on a **borrowed** (margin) balance are excluded from the base and reported for
manual review instead: repaying a currency loan is not clearly a transfer of a patrimonial element,
and none of the three texts settles it. A repayment that realized **exactly** zero is not reported at
all — there is nothing to review, because the amount is the same number under either answer. The test
is exact, not "prints as €0.00": a sub-cent result still moves the reported total, so a review row
showing `0.00` may still be carrying one. A zero-result conversion on a *held* balance is still
reported either way: that one is a disposal.

Under **Navarra** a conversion is also a transmission for the €3,000 exemption, and the tool records
only the result of one, never the amount converted — so a year with a conversion on a *held* balance
has its relief withheld. The conversion result itself is taxed in full: it is never relieved under
art. 39.5.d in its own right and never counts towards the article's global amount. See
[the exemption section](#the-navarra-3000-small-disposals-exemption) and
[Open interpretations §13](#13-the-navarra-3000-exemptions-global-amount--open).

## Other income

- **Stock grants / RSUs** are employment income and belong to the *general* base, which this tool
  does not compute. Each vest in the filing year gets a `Stock Grant` row carrying the vest-date
  value and a reminder to declare it separately; that same value is what costs the shares when they
  are later sold, so a vest with no FMV in the statement is flagged (it would otherwise be costed at
  zero and overstate the eventual gain).
- **Short positions** open at the end of the statement get no automatic treatment; they are listed
  for manual review.
- **Corporate actions** get a `Corporate Action` row each. Splits and renames are already applied to
  the FIFO queue and need nothing further; delistings, liquidations, spinoffs, scrip dividends and
  rights issues are **not** computed and each row says so.

## Modelo 109 / Modelo 100 / Modelo F-93 mapping

The CSV ends with a box mapping.

**Gipuzkoa publishes no blank numbered form** — the return is generated by Zergabidea — so the
numbers are read off the Hacienda Foral "Propuesta de autoliquidación" specimens, one per ejercicio
(`gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/<ejercicio>/propuesta-autoliquidacion`, retrieved
2026-08-11). Two sheets are involved and they number independently, so the keys name the sheet:
`MODELO_109_HOJA_<n>` for the Hoja de liquidación, `MODELO_109_ANEXO3_<n>` for Anexo 3.

The Hoja de liquidación numbering is stable from ejercicio 2019 through 2025:

| Casilla | Row |
|---|---|
| 28 | Rendimiento neto del capital mobiliario (base del ahorro) |
| 29 | Compensación de rendimientos negativos de ejercicios anteriores |
| 30 | Ganancias y pérdidas patrimoniales por transmisiones |
| 31 | Compensación de pérdidas de ejercicios anteriores |
| 33 | Base liquidable del ahorro |
| 38 | Cuota íntegra del ahorro |

The deduction block is **not** stable: NF 1/2025 renumbered it for ejercicio 2025.

| Row | 2023 / 2024 | 2025 onwards |
|---|---|---|
| Deducción por doble imposición internacional | 60 | **70** |
| Total deducciones | 63 | 73 |
| Cuota líquida | 64 | **74** |
| Pagos a cuenta del capital mobiliario | 67 | **77** |
| Total pagos a cuenta | 72 | 81 |

Two caveats the CSV states as warnings:

- **Anexo 3's internals are confirmed only through ejercicio 2024** and were demonstrably renumbered
  in 2025. The rendimientos íntegros (`06+16`) and gastos (`17`) rows are therefore emitted for
  ejercicios up to 2024 only; from 2025 the Hoja rows stand alone and the breakdown has to come from
  your own Anexo 3. Anexo numbering is a different space from the Hoja's — Hoja 28 is the rendimiento
  neto del capital mobiliario while Anexo 4's own 28 is ganancias — so never cross-read them.
- **No form exists for a year that has not been filed yet.** Ejercicio 2026 is filed in 2027, so its
  block carries the ejercicio-2025 numbers and says so.

For **Territorio Común** the rows carry the ejercicio-2025 numbers read from Anexo I of the Orden
HAC/277/2026 **consultation draft** — interest 0027, dividends 0029, gastos de administración y
depósito 0037, ganancias por transmisión de acciones cotizadas 0326-0340, base **imponible** del
ahorro 0460, deducción por doble imposición internacional 0588. The AEAT publishes a filing year's
form in the spring of the following one, so no enacted numbering exists yet.

Two of those rows carry a caveat the number alone does not:

- **0460 is the base imponible, not the liquidable.** LIRPF art. 50 puts the base liquidable del
  ahorro in casilla **0510**: 0460 minus whatever is left of the reducciones por tributación
  conjunta, pensiones compensatorias y anualidades por alimentos. The tool models none of those, so
  what it computes is 0460, and the label says so — a row labelled "liquidable" would send a filer
  one box further down the form than the figure belongs. Gipuzkoa's casilla 33 and Navarra's 815 do
  say *liquidable*, because their own forms use that word for the box at that point.
- **0326-0340 is the acciones-cotizadas block** (0327 one row per operation, 0339 the sum of gains,
  0340 the sum of losses). The tool posts the ganancias group's net figure, which also contains any
  foreign-currency conversion result — a transmisión under LIRPF art. 33, but of a different kind of
  element, belonging in the block for otros elementos patrimoniales. A Común year with a currency
  result therefore prints a warning naming the amount to split out; the tool does not split it,
  because the alternative block's numbering is not verified against an enacted form. Gipuzkoa's
  casilla 30 and Navarra's 706 are labelled "por transmisiones" without narrowing to shares, so the
  same figure is at home there.

For **Navarra** the rows carry the ejercicio-2025 numbers, read from the fully numbered Modelo F-93
the Boletín Oficial de Navarra publishes as Anexo I of each campaign's Orden Foral (ejercicio 2025 =
Orden Foral 24/2026, BON nº 66 of 07-04-2026). Keys are `MODELO_F93_<casilla>`.

| Casilla | Row |
|---|---|
| 031 | Dividendos y participación en beneficios (art. 28.a y b), importe íntegro |
| 037 | Intereses de cuentas y otros rendimientos por cesión de capitales propios |
| 047 | Gastos de administración y depósito, after the 3% ceiling |
| 050 | Rendimiento neto del capital mobiliario (`031 + 037 − 047`) |
| 706 | Incremento o disminución de la parte especial del ahorro por transmisiones |
| 1658-1672 | Incremento exento, otros supuestos — where the art. 39.5.d relief goes, one cell per transmission |
| 8808 / 809 / 8815 / 8809 | Apartado H1: positive transmissions saldo, own-group compensation, RCM losses crossed in, net |
| 8810 / 8825 / 8805 / 8840 | Apartado H2, the RCM mirror |
| 8816 | Apartado H3, saldo negativo procedente de transmisiones |
| 8850 | Apartado H4, saldo negativo del capital mobiliario |
| 8841 | Total parte especial del ahorro (`8809 + 8840`) |
| 815 (= 524) | Base liquidable especial del ahorro |
| 829 (= 527) | Cuota íntegra especial del ahorro |
| 572 | Deducción por doble imposición internacional |
| 818 / 8875 | Saldos negativos a compensar en los ejercicios siguientes |

The mapping is cross-checked against the form's own arithmetic — `8809 = 8808 − 809 − 810 − 8815`,
`8840 = 8810 − 8825 − 8835 − 8805`, `8841 = 8809 + 8840`, `050 = 031 + 037 − 047` — so a wrong casilla
shows up as a sum that does not close rather than as a plausible number.

Three things it deliberately leaves out:

- **810 and 8835** are the joint-return rows. The tool computes an individual return, so they are
  always zero.
- **569, 576 and 582** net the whole return, including the general part this tool does not compute.
  Casilla 569 in particular is the cuota líquida *before* the deducción por doble imposición, which
  is then subtracted through 575 — putting a savings-only net figure there would be wrong twice over.
- **613** is the transparencia fiscal internacional deduction, not the double-taxation one. The
  double-taxation credit is 572.

A filing year other than 2025 keeps these numbers under a warning: earlier campaigns share the
structure but their own numbering was not checked against a specimen, and no form exists for a year
that has not been filed yet. See
[Open interpretations §15](#15-navarra-casillas--verified-for-ejercicio-2025-open-beyond).

### Foreign withholding is not a retención

A **retención** is Spanish tax already withheld on your account (Modelo 109: pagos a cuenta casilla 67
through ejercicio 2024, 77 from 2025, and `07+22` on Anexo 3 while that sheet is emitted; Modelo 100
casilla 0597; Modelo F-93 casilla 030, which feeds 579). The tax an IB statement shows is withheld by
the *source* state, and it is relieved only through the deducción por doble imposición internacional.
The tool therefore emits **no** retenciones row and prints a warning naming the casilla the figure
does not belong in — entering it in both places claims the same tax twice on the same form.

## CSV output

See [`specs/002-spain-tax/contracts/csv-output.md`](../specs/002-spain-tax/contracts/csv-output.md)
for the full contract. In short: a 19-column transaction block where each capital gain is followed by
its per-FIFO-lot `Lot` rows (so the actualization multiplication reads line by line), then a
3-column `summary_key,label,value_eur` block holding the group totals, the compensation, the
carry-forward and deferral blocks, and the Modelo mapping. Comment lines start with `#`.

## HTML report (printable, A4 landscape)

Passing an `.html` output path instead of `.csv` writes a self-contained, printable report in
Spanish, modelled on the annual summaries brokers hand out:

```bash
investments tax-statement ib 2026 spanish-tax-2026.html
```

The report contains, in order (sections with nothing to say are omitted):

1. Title page with the regime, the four key figures, the disclaimer, the broker, the portfolio and
   the statement period
2. **Resumen para los formularios** — every casilla of the filer's own return (Modelo 109, Modelo
   100 or Modelo F-93), with the specimen each number was read from and the retenciones casilla the
   foreign withholding must *not* go in
3. **Cálculo del impuesto** — the two groups, the compensation, the savings scale bracket by
   bracket, the average rate and the double-taxation credit
4. **Resumen por actividad y categoría** and **Ganancias y pérdidas por valor** — what each activity
   and each security contributed, with the group it lands in
5. **Movimientos de efectivo** — dividends, withholding, interest and fees in original currency with
   the ECB rate
6. **Retenciones en origen** — withholding per country with the treaty-capped credit
7. **Ganancias y pérdidas patrimoniales (FIFO)** — one worksheet per disposal with the lots it
   consumed and, under Gipuzkoa, each lot's actualization coefficient and actualized cost
8. **Valores homogéneos** — the losses deferred this year, the ones reintegrated, and the ones still
   blocked at 31 December
9. **Operaciones con valores** — every buy and sell of the year in original currency
10. **Diferencias de cambio** — the per-currency FIFO ledger with each realization's treatment
11. **Posiciones abiertas a 31/12** — unsold purchase lots with their EUR cost
12. **Compensación y saldos pendientes** — each vintage's opening balance, what the year applied and
    what expires when, followed by the `taxes.spain` block the next return starts from
13. **Relación de valores** — symbol, ISIN, name, country, currency and category
14. **Avisos y observaciones** — the calculation warnings, the per-position notes, the vests, the
    corporate actions and the method's limits

All EUR amounts are the same figures as in the CSV and on the console (same rounding), in Spanish
notation (`1.234,56`); dates are `dd/mm/yyyy` everywhere except the configuration block, which uses
ISO because that is what the configuration reads.

### Converting to PDF

The page carries print CSS for A4 landscape (`@page`, repeated table headers, no row splits), so any
Chromium-based tool renders it:

```bash
# Chrome / Chromium headless
chromium --headless --print-to-pdf=spanish-tax-2026.pdf --no-pdf-header-footer spanish-tax-2026.html

# Playwright (Node): page.pdf({ path, format: 'A4', landscape: true, preferCSSPageSize: true })

# Gotenberg (the file must be named index.html)
curl --request POST http://localhost:3000/forms/chromium/convert/html \
  --form files=@index.html --form landscape=true --form preferCssPageSize=true \
  -o spanish-tax-2026.pdf
```

### Caveats

- The report is informational and does not replace the broker's official statements. Its own
  "Método y límites" list names the obligations it leaves alone — the general base, **Modelo 720**
  and the Impuesto sobre el Patrimonio — so a reader who never opens this document still learns of
  them.
- The **calculation warnings are in English**, shown verbatim under "Avisos del cálculo (texto
  literal)". They are one text for the console, the CSV and the report, so translating them here
  would let two surfaces say different things about the same figure.
- The **Acciones / Fondos e IIC** split reads `taxes.etf_classification` by ISIN, a key otherwise
  used by the German statement and optional here: no figure in the savings base depends on it. It
  groups the report and flags the payers the €1,500 Gipuzkoa exemption does not reach.
- The double-taxation credit uses the flat 15% treaty rate described above; there is no per-country
  treaty table.
- Open lots reflect the FIFO engine's view at the statement's last date; a statement extending past
  31 December already has later sales deducted (the report says so).
- The foreign-currency ledger **starts at zero**: no opening balance is carried in, so a statement
  that begins with currency already in the account shows a borrowed balance that is not one.
- The deferred-loss carry-out is a snapshot at 31 December. A repurchase window that closes after
  the statement ends is named in the report, not settled by it.
- Security names come from `instrument_names` in the portfolio config when set, otherwise from the
  Flex export's `description`, otherwise the symbol. The CSV keeps using the configured name or the
  bare symbol.
- Derivatives are skipped, as for the CSV.

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

### Navarra: what this tool does not compute

Every item below is a real Navarra obligation or rule that the savings-base computation deliberately
leaves alone. None of them is detectable from a broker statement.

1. **Foral Modelo 720 and Modelo 721.** Navarra runs its own informational declarations of assets
   abroad (Orden Foral 80/2013, as amended by OF 58/2026) and of virtual currencies held abroad
   (Orden Foral 29/2023), both filed 1 January – 31 March. Filing the state versions does not
   discharge them. This tool computes neither.
2. **Fund traspaso deferral.** Ley Foral 19/2021 brought the rollover regime into Navarra with a
   grandfathering rule in DT 29.ª for holdings acquired before 1 January 2022. The regime requires
   the transfer to run through the Spanish system, so it does not reach a foreign broker — but a
   filer with a pre-2022 Spanish-marketed holding should check DT 29.ª before assuming a disposal is
   taxable.
3. **Exit tax.** DA 46.ª taxes latent gains on a change of residence abroad above its own thresholds.
   A broker statement carries no residence history, so the tool cannot see the trigger.
4. **Related-party interest.** Art. 54.1.a moves interest on capital lent to a linked entity out of
   the savings base and into the general one, for the part exceeding three times that entity's
   equity (25% participation assumed where the link is not shareholder-based). Nothing in a broker
   statement identifies a linked entity.
5. **Joint returns.** Art. 74 lets members of a unidad familiar compensate each other's negative
   saldos, which is what the F-93's casillas 810, 8035, 8835 and 8895 are for. The tool computes an
   individual return and always leaves those boxes at zero.
6. **Personal and family minima.** Art. 62.9 applies them as credits against the quota rather than
   as reductions of the base, so the savings quota this tool reports is exactly `scale(base)`; the
   minima come off elsewhere on the return.

## Open interpretations

Every point where the tool had to choose a reading, what settles it, and what the tool says when a
case actually turns on it. Each runtime warning names this section.

### 1. Which valores-homogéneos window a listing takes — SETTLED, with edge cases

**Authority.** DGT CV V0778-25 (05-05-2025) and V0951-25 (30-05-2025), via
`petete.tributos.hacienda.gob.es` (retrieved 2026-08-11); Commission Implementing Decisions (EU)
2017/2320 (US), 2017/2318 (Australia), 2017/2319 (Hong Kong).

**The tool's reading.** Two months for every instrument. That is the DGT's own criterion for any
third-country venue covered by an in-force MiFID II art. 25(4)(a) equivalence decision, and following
a published criterion also shields the filer from penalties (LGT art. 179.2.d). For Gipuzkoa those
criteria are persuasive rather than binding: NF 3/2014 art. 43.g clones the state wording, but no
foral pronouncement exists.

**Edges that stay open.** Switzerland's decisions lapsed on 30-06-2019; the United Kingdom, Canada
and Japan have none. And V1872-25 (14-10-2025), while confirming that dual-listed lines of the same
class are homogeneous, expressly declines to say whether an ADR is homogeneous with its underlying
ordinary share — they carry different ISINs, so the tool treats them as different securities.

**What you see** when a repurchase falls outside the two months but inside the year on such a venue:

```text
# WARNING: the deduction below turns on which valores-homogéneos window the listing venue takes.
WASH_SALE_VENUE_REVIEW,SWCH sold 2026-03-10 — listed on EBS,900.00
```

**What to do.** The loss is deducted in full. If the amount matters, either check whether the venue is
covered by a decision that is in force for your filing year, or defer the amount by hand.

### 2. Where the window's edges fall — SETTLED, with edge cases

**Authority.** Código Civil art. 5.1 (via LGT art. 7.2); STS 552/2022 (10-05-2022, RC 1874/2021),
STS 02-07-2020 (RC 3780/2019), STS 02-04-2008 (rec. 323/2004); Ley 39/2015 art. 30.4.

**The tool's reading.** Two calendar months de fecha a fecha, both ends inclusive, clamped to the last
day of a short month. No authority applies that arithmetic to art. 33.5.f with concrete dates, so a
deferral decided by a single day is named.

**What you see**, in the sale row's `notes`, on the console and in the log:

```text
€900.00 of the AAPL loss of 2026-03-10 turns on window-boundary arithmetic: homogeneous securities
were acquired on the window's own terminal day, … The tool puts that edge on 2026-05-10; the other
reading puts it on 2026-05-09. …
```

**What to do.** Nothing is wrong with the figure; it is simply the one that hangs on the convention.
Take advice before filing if the amount is material, and keep the dates — they are what an inspector
would ask about.

### 3. Which broker fees are deductible — SETTLED per type, two types open

**Authority.** DGT V2117-19, V2629-13, V1047-16, consulta 03-04-1998; AEAT Manual de Renta cap. 5
(2024/2025). Per-type table under [Custody and administration fees](#custody-and-administration-fees).

**The tool's reading.** Under Común, custody / administration / depósito is deducted, dividend
collection and corporate-event handling are deducted at medium confidence, trading commissions adjust
the acquisition and transmission values instead, and management / advisory / performance, market data
and cash-movement fees are not deductible. A foreign broker charging the fee does not change the
answer (medium confidence: no consulta is on point, but the wording is not territorially limited).
Under Gipuzkoa nothing is deductible whatever the type is, so no question arises there.

**Still open**, and therefore not deducted: inactivity / minimum-activity / maintenance /
connectivity fees, standalone currency-conversion fees, and securities transfer-out (traspaso) fees.

```text
€27.00 of inactivity, minimum-activity or maintenance fee on 2026-09-15 is NOT deducted from the
savings base: no DGT doctrine settles whether it is a gasto de administración y depósito under
LIRPF art. 26.1.a. …
```

**What to do.** Not deducting overstates the tax rather than understating it. Consult a gestor if the
amount is material, and deduct it by hand on the return if advised to.

### 4. Modelo 109 casillas — VERIFIED 2023–2025, open beyond

**Authority.** Hacienda Foral de Gipuzkoa "Propuesta de autoliquidación" specimens per ejercicio
(`gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/<ejercicio>/propuesta-autoliquidacion`, retrieved
2026-08-11).

**The tool's reading.** Hoja de liquidación numbers as published, with the NF 1/2025 renumbering of
the deduction block applied from ejercicio 2025. No casilla is guessed.

**Still open.** Anexo 3's internal numbering is confirmed only through ejercicio 2024 and was
renumbered in 2025, so the íntegros / gastos breakdown rows stop there; and no form exists for a year
that has not been filed yet, so ejercicio 2026 gets the 2025 layout:

```text
# WARNING: no form is published for ejercicio 2026 yet — it is filed in 2027.
# WARNING: the rendimientos íntegros / gastos breakdown lives in Anexo 3,
```

**What to do.** Check each casilla against your own proposal in Zergabidea — it is generated for you,
and it is the authoritative numbering for your year.

### 5. Modelo 100 casillas — DRAFT SOURCE

**Authority.** Anexo I of the Orden HAC/277/2026 **consultation draft**. The AEAT publishes a filing
year's form in the spring of the following one, so no enacted numbering exists yet. A `# WARNING`
above the box rows says so. Verify against the published form before filing.

### 6. The €1,500 exemption and fund distributions — OPEN, by data limit

NF 3/2014 art. 9.24 does **not** cover distributions from instituciones de inversión colectiva, and a
broker statement does not distinguish a fund distribution from a company dividend. The tool exempts
both and names every payer it exempted:

```text
# WARNING: €<x> of dividends were exempted under NF 3/2014 art. 9.24. The exemption
# does NOT cover distributions from instituciones de inversión colectiva (funds,
```

**What to do.** Check each payer named and reduce the exemption by hand if any of them is a fund.

### 7. Foreign-currency results on a borrowed balance — OPEN

Repaying a currency loan is not clearly a transfer of a patrimonial element, and none of NF 3/2014,
the LIRPF and the TRLFIRPF settles it. Those results are **excluded** from the savings base and
reported for manual review instead (`FX Borrowed (review)` rows and `SUMMARY_FX_BORROWED_REVIEW`),
with a console warning.

Under Navarra the exclusion has a second effect: because those results never reach the ganancias
group, they do not trigger the art. 39.5.d suppression either, so a year whose only conversions were
on a borrowed balance can still be granted the small-disposals relief on a securities-only total. See
[§13](#13-the-navarra-3000-exemptions-global-amount--open).

### 8. Fecha de transmisión — SETTLED, no warning

Trade date, not settlement date (LIRPF art. 14.1.c; NF 3/2014 art. 57.1.b; DGT V0152-26). It decides
which tax year a December sale falls in.

### 9. Deferred-loss quantities across a split — DOCUMENTED LIMITATION

A `deferred_losses` entry carried in from a prior return is taken in the units that return reported. A
split between that acquisition and the current statement is **not** applied to it; adjust
`blocked_quantity` by hand if one happened. Quantities inside a single statement are normalized.

### 10. Repurchase window still open when the statement ends — DATA COVERAGE

Not an interpretation but the same kind of risk: a loss whose +2-month window runs past the
statement's last date is deducted in full and flagged (`WASH_SALE_WINDOW_OPEN`). Export a statement
that extends at least two months past year end and re-run.

### 11. Which Navarra saldos may cross the groups — OPEN

**Authority.** TRLFIRPF art. 54.2 (wording of Ley Foral 23/2015), read against art. 54.3. No
Hacienda Foral de Navarra manual or consulta on the point was located (searched 2026-08-12).

**The tool's reading.** Art. 54.2 opens its 25% cross-offset for a negative **current-year** result:
"si el resultado fuese negativo, su importe se compensará con el saldo positivo resultante de la
letra b) … con el límite del 25 por 100 de dicho saldo positivo". What is left then carries four
years "en el mismo orden establecido en los párrafos anteriores", and the tool reads "el mismo
orden" as repeating the whole order — cross included — for a saldo carried in from an earlier year.
The current year's own negative is served first, so a carried saldo only ever takes what is left of
the allowance, and the amount reported is exactly what the reading is responsible for.

**What you see** when a carried saldo is what crossed:

```text
# WARNING: €5625.00 of prior-year negative savings-base saldos was set against the other
# group under TRLFIRPF art. 54.2. … On the narrower reading the amount would stay pending
# and the savings base would be €5625.00 higher.
```

**What to do.** The narrow reading gives a higher base and more tax this year, and leaves the saldo
pending for later. If the amount matters, take advice before filing.

### 12. What the Navarra 3% fee ceiling is measured on — OPEN, by data limit

**Authority.** TRLFIRPF art. 32.1.a: gastos de administración y depósito de valores negociables
"con el límite del 3 por 100 de los ingresos íntegros, que no hayan resultado exentos, procedentes de
dichos valores".

**The tool's reading.** The ceiling is measured on **dividend** income. Interest credited on a broker
cash balance is a rendimiento from the cesión a terceros de capitales propios (art. 29), not income
from a valor negociable, so it does not raise the ceiling. A broker statement does not separate a
bond coupon from cash-account interest, so a filer holding bonds has a larger ceiling than the tool
computes.

**What you see** when the ceiling bites:

```text
# WARNING: €18.00 of otherwise deductible custody and administration fees was NOT deducted:
# TRLFIRPF art. 32.1.a caps them at 3% of the non-exempt gross income from the securities…
```

**What to do.** Not deducting overstates the tax rather than understating it. If part of your
interest is a coupon on a negotiable security, raise the ceiling by hand.

### 13. The Navarra €3,000 exemption's global amount — OPEN

**Authority.** TRLFIRPF art. 39.5.d.

**The tool's reading.** Condition 1.º speaks of "el importe global de las citadas transmisiones",
condition 2.º of "el importe global de la transmisión" — singular. The tool reads both as the same
year-global figure: 1.º has already fixed "importe global" as the year total, and a per-disposal
numerator against a global denominator is incoherent. The two readings differ only in a
multi-disposal year whose disposals have unequal gain-to-proceeds ratios. Proceeds are counted for
every disposal, gain- or loss-making, because 1.º measures the transmissions; only positive
integrable results feed the increment, because a loss is a *disminución*.

**What the choice is worth.** A year that sells 6 shares for €1,620 (gain €1,080, more than half its
own proceeds) and 4 for €450 (gain €90, less than half of its own) has `G = 2,070` and `I = 1,170`.
Year-global exempts `min(1,170, 1,035) = 1,035`; per-disposal exempts `810 + 90 = 900`. The €135 of
base — €27.00 of Navarra tax — is the whole difference, and it exists because one year-wide
denominator lets the second sale's unused headroom shelter part of the first sale's excess.

**Also open: what 2.º's 50% ceiling is measured on.** The increment `I` counts only the year's
*incrementos*, but the ceiling it is compared against is half of the **global** amount — every
transmission, gain- and loss-making alike. The two figures therefore rest on different populations,
so a loss-making sale widens the shelter available to the year's gains. The tool reads it that way
because 2.º says "el importe global de la transmisión" and 1.º has already fixed "importe global" as
the year total; reading one phrase two ways inside a single letra would need an argument the article
does not give.

This is the round's one choice whose **failure direction is less tax**. A year that
sells one holding for €1,800 at a €1,620 gain and another for €450 at a €270 loss has `I = 1,620`
and passes 1.º under either reading (€2,250 and €1,800 are both under €3,000). Half the global
amount is €1,125 and half the gain-making sale's own proceeds is €900, so the tool exempts €1,125
where the narrower reading exempts €900: €225 of base, €45.00 of Navarra tax, in the filer's favour.

**What to do.** If a loss-making disposal is what lifts your exemption above half the gain-making
disposals' proceeds, the relief the tool grants rests on this reading. Re-run the arithmetic with the
narrower denominator before filing, or ask Hacienda Foral de Navarra.

**Also open: gross or net of the sell commission.** The global amount is built from the same
`proceeds_eur` the gain is built from, which is revenue **less the sell commission**. Art. 41.2 takes
"los gastos y tributos … en cuanto resulten satisfechos por el transmitente" out of the *valor de
transmisión*, and the F-93's per-transmission column 651 is labelled *Valor de transmisión* — so net
is the reading the form invites. But art. 39.5.d says "el importe global de las citadas
transmisiones" rather than "el valor de transmisión", and art. 41.3 defines the *importe real del
valor de enajenación* as "el efectivamente percibido", which reads gross.

The choice only bites when a commission straddles a threshold, and it is **not uniformly
conservative**: a net figure is smaller, which makes 1.º easier to pass (more relief) but lowers
2.º's 50% ceiling (less relief). A single sale of €3,060 gross with a €90 commission is €2,970 net:
net exempts €1,485, gross exempts nothing because 1.º fails.

**What to do.** If a sell commission puts the year's gross proceeds above €3,000 while the net figure
is at or under it, the relief the tool grants rests on the net reading. Re-run the arithmetic on the
gross figure before filing, or ask Hacienda Foral de Navarra.

**Also open, by data limit.** A foreign-currency conversion is a transmission too (art. 54.1.b), but
the shared FX FIFO records only its result, never the amount converted, so the global amount cannot
be measured in a year that has one. The exemption is then **withheld** rather than granted on an
understated total — which overstates the tax. Above €3,000 of securities proceeds nothing is
withheld and nothing is said: the missing conversions can only add to a total that has already
failed the test.

```text
# WARNING: The year's securities transmissions came to €2070.00, under the €3,000 that
# TRLFIRPF art. 39.5.d exempts, but the exemption was NOT applied: the year also contains
# foreign-currency conversions…
```

Three boundaries on that suppression, each deliberate:

- **A year with no transmission *gain* is silent.** The relief is `min(I, 50% × G)`, so a year whose
  securities transmissions produced no incremento — because there were none, or because every one of
  them made a loss — would have been relieved of exactly zero however the conversions were counted,
  whatever they did to `G`. Announcing a withheld relief there reports a counterfactual that is
  closed. It is closed **under the tool's scope limit**, not absolutely: the tool feeds only
  securities results into `I`, and art. 54.1.b makes the conversion a transmisión, so whether a
  conversion's own increment belongs in `I` is an open legal question rather than a settled one.
  Nothing the year-by-year message could say would repair that, so the limit is stated permanently
  instead — in this entry and on the CSV's own `SUMMARY_GYP_FX` row under Navarra, on every run.
- **Only results on a *held* balance suppress it.** Conversion results on a **borrowed** balance are
  parked in the manual-review bucket of §7 and never reach the ganancias group, so they do not
  trigger the suppression either. Their importe is just as unmeasurable, so a year whose only
  conversions were on a borrowed balance can still be granted the relief on a securities-only total.
  The asymmetry follows from §7's decision to keep borrowed-balance results out of the base
  entirely; if you settle §7 the other way, this suppression has to widen with it.
- **Nothing is withheld above €3,000**, as above.

**What to do.** Add the converted amounts — held and borrowed alike — to the securities proceeds. If
the total is still at or under €3,000, claim the exemption by hand; if a borrowed-balance conversion
pushes it over, the relief the tool granted is too generous.

### 14. The Navarra credit's tipo medio efectivo — OPEN, by scope

**Authority.** TRLFIRPF art. 67.2: the tipo medio efectivo is `100 × cuota líquida / base liquidable`,
split general vs ahorro, expressed with two decimals.

**The tool's reading.** The article divides *cuota líquida*, and the tool models no savings-side
deduction — Navarra's personal and family minima are quota credits under art. 62.9, applied against
the general part — so within its scope savings cuota líquida equals savings cuota íntegra and the
average rate it computes is exact. A filer who does carry a savings-side deduction has a **lower**
tipo medio and therefore a lower credit ceiling than the tool reports.

**What to do.** If your return carries a deduction against the savings quota, recompute the ceiling
from your own cuota líquida.

### 15. Navarra casillas — VERIFIED FOR EJERCICIO 2025, open beyond

**Authority.** The fully numbered Modelo F-93 the Boletín Oficial de Navarra publishes as Anexo I of
each campaign's Orden Foral; ejercicio 2025 = Orden Foral 24/2026, BON nº 66 of 07-04-2026 (retrieved
2026-08-12).

**The tool's reading.** The ejercicio-2025 numbering as published, cross-checked against the form's
own arithmetic (`8809 = 8808 − 809 − 810 − 8815`, `8840 = 8810 − 8825 − 8835 − 8805`,
`8841 = 8809 + 8840`, `050 = 031 + 037 − 047`). Casillas 810 and 8835 are the joint-return rows and
are never emitted; 569, 576 and 582 net the whole return, including the general part this tool does
not compute, so they are not emitted either.

**A stated convention: 8816 and 8850 carry a positive magnitude.** The form defines both as *saldos
negativos* — sums that are below zero by construction, which is why the apartado exists at all — and
the CSV prints `max(0, −gyp_net)` and `max(0, −rcm_net)`, never a leading minus. It is a choice about
this tool's output rather than a reading of the statute or of the form, and it is here so it cannot
drift silently: the row labels say "importe en positivo" and the CSV contract's row descriptions
(`specs/002-spain-tax/contracts/csv-output.md`) repeat it. Check the sign your own campaign's form
expects before transcribing.

**Still open.** Earlier campaigns use the same structure but their numbering was not checked against
a specimen, and no form exists for a year that has not been filed yet:

```text
# WARNING: the ejercicio-2024 form has the same structure but its own
# numbering was not checked against a specimen. Verify every casilla.
```

**What to do.** Check each casilla against the Anexo I of your own campaign's Orden Foral.

### 16. Pre-1994 lots under Navarra — NOT COMPUTED, by data limit

**Authority.** TRLFIRPF DT 7.ª.

**What it does.** For an element acquired **before 31 December 1994**, the part of the gain generated
before 31 December 2006 is reduced — 25% per year of holding beyond two for listed shares — and is
not taxed at all once the holding period at 31 December 1996 passed five years. Navarra put **no
€400,000 lifetime cap** on the relief, unlike the state regime.

**Why it is not computed.** DT 7.ª.3 measures the pre-2006 part against the element's value for the
2006 Impuesto sobre el Patrimonio, which a broker statement does not carry. The tool therefore prices
such a lot without the relief and says so, naming each lot and its date:

```text
# WARNING: The year's disposals consumed FIFO lots acquired before 31 December 1994
# (AAPL acquired 1993-06-15). … The gains above are therefore OVERSTATED.
```

Note the cut-off is the article's own: an acquisition **on** 31 December 1994 is outside it. The
warning is Navarra-only — the Gipuzkoa and state equivalents exist but were not researched, so the
tool says nothing rather than citing the wrong statute for them.

**What to do.** Take advice and compute the reduction by hand from your 2006 wealth-tax valuation.

### 17. Debt instruments are not in the statement at all — DOCUMENTED LIMITATION

TRLFIRPF art. 29 (and LIRPF art. 25.2) classify the result of transferring a debt instrument as
**rendimiento del capital mobiliario**, not as a ganancia patrimonial. The question never arises
here: the shared Interactive Brokers parser discards every instrument whose `assetCategory` is not
`STK`, with its own warning, so a bond disposal never reaches the Spanish pipeline and appears in
neither savings-base group.

```text
Skipping non-stock instrument <symbol> (assetCategory BOND): the tool computes taxes for
stocks only, so derivatives and other categories are out of scope.
```

**What to do.** Declare bond disposals by hand, as RCM rather than as ganancias.

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

### "taxes.spain.coefficients is set but the regime is navarra"

Navarra has no actualization: TRLFIRPF art. 41 computes the gain from the acquisition value as paid.
Remove the block, or switch the regime if you meant to file in Gipuzkoa.

### Missing exchange rates

A conversion failure names the date and currency. It usually means the ECB published no reference
rate for that date; check that the date is a TARGET business day.

## Sources

Retrieved 2026-08-11 unless noted.

| Rule | Source |
|---|---|
| Navarra: savings scale (art. 60), compensation order (art. 54), fee ceiling (art. 32.1.a), small-disposals exemption (art. 39.5.d), FIFO (art. 43.2), wash sale (art. 39.6), no actualization (art. 41), credit (art. 67), abatement (DT 7.ª) | Decreto Foral Legislativo 4/2008 consolidated text, `lexnavarra.navarra.es` (retrieved 2026-08-12) |
| Navarra: Modelo F-93 casillas for ejercicio 2025 | Anexo I of Orden Foral 24/2026, BON nº 66 of 07-04-2026 (retrieved 2026-08-12) |
| Navarra: foral Modelo 720 and Modelo 721 | Orden Foral 80/2013 (mod. OF 58/2026); Orden Foral 29/2023 |
| Gipuzkoa savings scale 2026 (art. 76.1), and that NF 1/2025 leaves art. 9.24 alone | NF 1/2025 full text (`primeralecturaediciones.com/documentos_diana/LEYES_2025/GUIPUZKOA/NF_1_2025_Gipuzkoa_IRPF.pdf`) |
| Fungible-crypto FIFO wording in art. 47.2; art. 9.24 untouched | NF 2/2025 de 24 de noviembre (`primeralecturaediciones.com/documentos_diana/LEYES_2025/GUIPUZKOA/GIPUZKOA_NF2_2025.pdf`) |
| €1,500 dividend exemption, art. 9.24, item 24 verbatim with its exclusions | Diputación Foral de Gipuzkoa, Modelo 109 "Exenciones", ejercicios [2023](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2023/exenciones) / [2024](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2024/exenciones) / [2025](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2025/exenciones) — **OFFICIAL** |
| Custody fees not deductible in Gipuzkoa (art. 39) | Manual de Renta, cap. 4 §4.5 (`gipuzkoa.eus/documents/2456431/80349127/04+-+Rend+capital+mobiliario.pdf`) — **OFFICIAL** |
| Actualization coefficient tables (DF 58/2023, DF 61/2024, DF 27/2025) | Boletín Oficial de Gipuzkoa — **OFFICIAL** |
| Común two-phase compensation order and the joint 25% allowance | AEAT Manual Práctico de Renta, cap. 12 |
| The credit's rate limb takes **rentas netas** | TEAC resolución RG 00/08643/2023, 20-10-2025, unificación de criterio (binding per LGT art. 239.8) |
| The average rate is expressed with two decimals and used rounded | NF 3/2014 art. 76.2 / LIRPF art. 80.2; AEAT Manual Renta cap. 18 worked example (16,60% × 6.000 € = 996 €) |
| Fecha de transmisión = trade date | LIRPF art. 14.1.c / NF 3/2014 art. 57.1.b; DGT V0152-26 |
| Valores homogéneos = same issue and same rights (so same ISIN) | RIRPF art. 8; DF 33/2014 art. 47; DGT V0796-26 |
| Reintegration keyed to the recompra pool; releasing transfer must be definitive | DGT V0913-08; DGT V3282-18 |
| Modelo 100 box numbers, ejercicio 2025 | Anexo I of the Orden HAC/277/2026 **consultation draft** — no enacted numbering exists yet |
| State savings scales, LIRPF arts. 26, 33, 35, 37, 46, 49, 66, 76, 80 | BOE consolidated Ley 35/2006 — **OFFICIAL** |

Two questions stay open and are marked in the code:

- the keyword list that recognises a custody fee (art. 26.1.a names the *service*, not the wording a
  broker uses) — an unrecognised fee is reported but not deducted;
- whether the two-month window's endpoints are themselves inside it. Treated as inclusive, which
  keeps the Código Civil art. 5.1 "de fecha a fecha" reading and defers more rather than less.
