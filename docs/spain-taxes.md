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
summary. The output path is an optional positional argument: without it you get the summary alone.

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
filer from penalties (LGT art. 179.2.d). NF 3/2014 art. 43.g clones the state wording and interprets
the same EU concept, but no foral pronouncement exists: for a Gipuzkoa filer these criteria are
persuasive, not formally binding.

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

- **Unlisted securities** (art. 43.h / art. 33.5.g) use a **one-year** window, not two months. Not
  implemented.
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

### Both regimes

A balance whose fourth year has passed is dropped with a warning rather than carried — next year's
config must not claim an offset the tax office will refuse. A balance older than the window in the
config is a hard error, not a silent skip.

Simple example with no prior-year balances, RCM −2,000 against GyP +6,000:

| | Cross-offset | GyP taxable | Carried forward |
|---|---|---|---|
| Territorio Común | min(2,000, 25% × 6,000) = 1,500 | 4,500 | RCM 500 |
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

A broker statement carries only a free-text description, so the tool matches on it. Under Común each
type is treated as the DGT classifies it:

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

### Interest paid on a margin loan

IB reports margin interest as a **negative** "Broker Interest Paid" accrual in the same ledger as the
credit interest, so a naive sum would net it off your RCM income. Neither regime allows that: LIRPF
art. 26.1.a reaches only administration and custody of negotiable securities, and NF 3/2014 art. 39
is narrower still. The tool reports each paid-interest row without a savings group, sums them into
`SUMMARY_RCM_INTEREST_PAID`, and leaves the RCM result untouched.

## Foreign-currency gains

A currency conversion transfers a patrimonial element, so its result joins the **ganancias** group.
Balances are tracked with a signed-inventory FIFO over the cash ledger.

Results realized on a **borrowed** (margin) balance are excluded from the base and reported for
manual review instead: repaying a currency loan is not clearly a transfer of a patrimonial element,
and neither the foral nor the state text settles it.

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

## Modelo 109 / Modelo 100 mapping

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
depósito 0037, ganancias por transmisión de acciones cotizadas 0326-0340, base liquidable del ahorro
0460, deducción por doble imposición internacional 0588. The AEAT publishes a filing year's form in
the spring of the following one, so no enacted numbering exists yet.

### Foreign withholding is not a retención

A **retención** is Spanish tax already withheld on your account (Modelo 109: pagos a cuenta casilla 67
through ejercicio 2024, 77 from 2025, and `07+22` on Anexo 3 while that sheet is emitted; Modelo 100
casilla 0597). The tax an IB statement shows is withheld by the *source* state, and it is relieved
only through the deducción por doble imposición internacional. The tool therefore emits **no**
retenciones row and prints a warning naming the casilla the figure does not belong in — entering it
in both places claims the same tax twice on the same form.

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

Repaying a currency loan is not clearly a transfer of a patrimonial element, and neither NF 3/2014 nor
the LIRPF settles it. Those results are **excluded** from the savings base and reported for manual
review instead (`FX Borrowed (review)` rows and `SUMMARY_FX_BORROWED_REVIEW`), with a console warning.

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

## Sources

Retrieved 2026-08-11 unless noted.

| Rule | Source |
|---|---|
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
