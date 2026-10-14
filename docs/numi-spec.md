# Numi 3.35 calculation engine: behavioural spec for Numero

Source: clean-room analysis of `Numi.app` 3.35 (x86_64 slice of `NumiUI.framework`, Swift module `NumiEngine`).
Audience: the Rust implementer. This file describes behaviour and constants, not Numi's code.

Confidence tags:

- [obs] observed directly in the binary (Ghidra function `FUN_xxxxxx` or address in `NumiUI.x86_64`, or a resource file).
- [inf] inferred from structure, naming or documented Numi behaviour. Verify with a test on a real Numi install.
- [unk] not settled. A decision is needed (see Open questions at the end).

Evidence files:

- Ghidra dump: `/home/psoldunov/re/numi/dump/decomp.c` (search `//==== FUNC FUN_<addr>`).
- Decoded Swift short-string immediates: `/home/psoldunov/re/numi/dump/decoded.tsv` (func_addr, line, literal). Swift stores strings of 15 bytes or less inline as integers, so `strings.tsv` misses them. Most of the evidence below comes from this index.
- Helper scripts in `/home/psoldunov/re/numi/scripts/`: `skel.py <addr> full` (function with string literals annotated), `units2.py` (unit table extractor), `macho.py <addr>` (decodes static Swift `[String]` arrays), `ops.py <addr>` (NSDecimalNumber selector sequence of a function).
- Vocabulary: `Contents/Resources/NumiEngine_NumiEngine.bundle/Contents/Resources/en.lproj/*.strings`, written `L/<File>.strings` below.

---

## 0. Engine architecture

- Numbers are decimal, not binary floating point [obs]. All arithmetic uses `NSDecimalNumber` (38-digit mantissa, base-10 exponent -128..127). Every arithmetic call passes an `NSDecimalNumberHandler(roundingMode: .up (2), scale: 38, raises nothing)` (e.g. `FUN_00062670`, `FUN_000ce960`). In practice that means 38 significant digits, rounding up at the 38th decimal place, and no exceptions: overflow, underflow and divide-by-zero give NaN.
  - Rust: use a decimal type with at least 28-38 significant digits (`rust_decimal` has 28; `bigdecimal` is unbounded) and keep a NaN / "no result" state.
- Transcendental functions and a few operators go through `f64` [obs]: trig, hyperbolic, `ln`, `log`, `root`, `cbrt`, `sqrt`, `fact` (`tgamma`), `mod`, non-integer `^` (`pow`), the degree ratio (π/180 as a double), and `pi` itself (`NSDecimalNumber(double: M_PI)`). Their results are converted back to decimal from the double, so they carry about 15-17 significant digits.
- A Value is a number plus a string-keyed attribute bag (`Value.attributes`) [obs]. Keys seen: `decimal` (the number), `unit` (unit id), `nsystem` (`hex`/`oct`/`bin`/`sci`, output radix), `date`, `time_zone`, `operation`, `opPriority`, `scale`, `percent`, `geo_name`. A value with no attributes is "empty" (the line has no result). `sum`/`avg` depend on this, see §5.
- Per-line pipeline [obs, names from symbols]: extractors tokenise the line into typed elements; matchers group tokens into values and expressions (`DecimalFollowedByUnitMatcher`, `UnitFollowedByDecimalMatcher`, `UnitAroundDecimalMatcher`, `ScaledExpressionMatcher`, `FunctionExpressionMatcher`, `ExplicitUnitConversionMatcher`, `TimePrefixedByTimeZoneMatcher`, `GeoNameSequenceMatcher`); evaluators fold operators by priority (§2). The document is evaluated top to bottom with one shared `Context` (variables, `prev_result`, `prev_results`, `grand_total`, `current_time`, `ppi`, `em`, constants).
- Initialisation order (`FUN_00063ce0`) [obs]: operators (`FUN_0004ea10`), operations (`FUN_0004ff50`), extractors (`FUN_0004c1f0`), matchers/evaluators (`FUN_0004b750`), formatters, units (`FUN_00051d60`), functions (`FUN_0004db90`), currency (`FUN_000476b0`), crypto (`FUN_00048af0`), dates (`FUN_00049a10`), scales (`FUN_00051370`), constants (`FUN_00046370`), CSS (`FUN_00046930`).

---

## 1. Units

### 1.1 Unit model [obs: `Unit` ivars, e.g. `FUN_00052480`]

Each unit has `id`, `phrases` (input words), `baseUnitId`, `baseRatio` (decimal), `format` (output template), `isSiUnit` (gets SI-prefixed siblings), `internationalSymbol` (English `.symbol`), `localizedSymbol` (current-locale `.symbol`), and optional `toBaseValueConverter` / `fromBaseValueConverter` for non-linear or context-dependent units.

- Same-base conversion: `in_base = value * baseRatio`, `in_target = in_base / target.baseRatio`. The ratio is "base units per one of this unit" (inch = 0.0254, base `m`).
- Temperature, CSS and currency use converter functions instead of the ratio.
- Phrases come from `<id>.variants` in `L/<Category>.strings` (comma-separated). When the UI locale is not English, the English table is merged in as well [obs: `FUN_0007e710` param 5]. Units also get lower-cased copies of their phrases [inf: param 6 calls `lowercased()`]. Phrase collisions log `Duplicate unit for phrase '…': previously '…', adding as '…'` (`FUN_0009f630`) [obs]; which one wins is [unk].
- Output: `<id>.format`. If the template contains `%@`, the number replaces it (`$%@` → `$5`). Otherwise the output is `"<number> <format>"` (`m` → `5 m`) [obs: `FUN_000ac360` checks for `%@` and prepends `"%@ "`]. There are no plural forms anywhere.

### 1.2 Category tables

Ratios are exact decimal strings parsed with `NSDecimalNumber(string:)` unless noted.

Length (`FUN_00052480`) [obs], base `m`:

| id | ratio (m) | SI prefixes | notes |
|---|---|---|---|
| `m` | 1 | yes, negative ones included | base; `.symbol` = `m` |
| `inch` | 0.0254 | no | format `″` |
| `foot` | 0.3048 | no | |
| `yard` | 0.9144 | no | |
| `mile` | 1609.344 | no | format `mi.` |
| `point` | 0.00035277777 | no | truncated, not 0.0254/72 |
| `cable` | 185.3184 | no | |
| `league` | 4828.032 | no | |
| `furlong` | 201.168 | no | |
| `nautical_mile` | 1853.184 | no | Admiralty mile, not the international 1852 m |
| `chain` | 20.1168 | no | |

Area (`FUN_00056670`) [obs], base `m2`:

| id | ratio (m²) |
|---|---|
| `acre` | 4046.86 (rounded; the international acre is 4046.8564224) |
| `hectare` | 10000 |
| `are` | 100 |
| `ping` | 3.306 |
| squares of length units | see §1.3 |

Volume (`FUN_00057cc0`) [obs], base `m3`:

| id | ratio (m³) | SI |
|---|---|---|
| `litre` | 0.001 | yes, negative ones included (ml, cl, dl, hl, kl…); `.symbol` `l` |
| `cup` | 0.000236588237 | no |
| `quart` | 0.000946352946 | no |
| `pint` | 0.000473176473 | no |
| `gallon` | 0.00378541178 | no |
| `tablespoon` | 0.0000147867648 | no |
| `teaspoon` | 0.00000492892159 | no |
| cubes of length units | see §1.3 | |

`L/Volume.strings` also defines `cbinch.variants` (`c.i., CI, cui, cu inch`) and `cbcm.variants` (`cc, ccm`), but no code reads those keys [obs: no immediate for `cbinch`/`cbcm` anywhere]. Numi 3.35 does not wire them up. See Open questions.

Weight (`FUN_0005f820`) [obs], base `gram`:

| id | ratio (g) | SI |
|---|---|---|
| `gram` | 1 | yes, negative ones included (mg, kg…) |
| `tonne` | 1000000 | yes (kt, Mt, and the negative ones) |
| `carat` | 0.2 | no |
| `pound` | 453.59237 | no |
| `centner` | 100000 | no |
| `ounce` | 28.349523125 | no |
| `stone` | 6350.29318 | no |

Time (`FUN_0005d6c0`; ratios from the `swift_once` initialisers `FUN_000d2de0`…`FUN_000d2ed0`) [obs], base `second`:

| id | ratio (s) |
|---|---|
| `second` | 1 (SI, negative ones included: ms, µs, ns…) |
| `minute` | 60 |
| `hour` | 3600 |
| `day` | 86400 |
| `week` | 604800 |
| `month` | 2592000 (30 days) |
| `year` | 31536000 (365 days) |

Data (`FUN_0005a600`) [obs], base `bit`:

| id | ratio (bit) | prefixes |
|---|---|---|
| `bit` | 1 | positive SI only (k, M, G…) plus IEC (Ki, Mi…) |
| `byte` | 8 | positive SI only plus IEC |

SI prefixes on data are decimal (1 kB = 1000 B); IEC prefixes are binary (1 KiB = 1024 B). `bit.symbol` = `b`, `byte.symbol` = `B`.

Angle (`FUN_0005cdb0`) [obs]: base `radian`. `degree` ratio = 0.017453292519943295, which is π/180 stored as a double at `0x109230` and converted to decimal.

Temperature (`FUN_0005c030`; converters `FUN_000ce960`, `FUN_000ceb20`, `FUN_000cff40`, `FUN_000d0260`) [obs]. Base `kelvin`, everything through converters:

- celsius → kelvin `K = C + 273.15`; kelvin → celsius `C = K - 273.15`.
- fahrenheit → kelvin `K = (F + 459.67) * 5 / 9`; kelvin → fahrenheit `F = K * 9 / 5 - 459.67`.
- C↔F goes through kelvin, with decimal rounding at 38 digits.
- Phrases include bare `C`, `F`, `K` (`L/Temperature.strings`).

CSS (`FUN_00046930`; converters `FUN_000d1090`, `FUN_000d13d0`, `FUN_000cf600`, `FUN_000cfa20`) [obs]. Both units have base `m`:

- `pixel`: `px = m * (ppi / 0.0254)`, `m = px / (ppi / 0.0254)`. `ppi` is read from the context variable `ppi` at conversion time.
- `em`: `m = em * (variable em, converted to m)`; `em = m / (variable em in m)`.
- Startup defaults, set as ordinary context variables: `ppi = 96`, `em = 16 px`. `em` also gets the context flag `hidden_var_em = true` (hidden from the variable list) [obs]. A document can override them with `ppi = 72` or `em = 20 px` [inf: they are ordinary variables].

Percent (`FUN_00061f50`) [obs]: pseudo-unit `percent`, base `percent`, phrases `%, percent, percents, pct, pct.`, format `%%`.

Currency (`FUN_000476b0`; crypto `FUN_00048f90`) [obs]: one unit per code in `L/Currency.strings` (ISO 4217 codes plus `BTC`), base `USD`, converted through functions rather than ratios:

- to USD: `usd = amount / rate[code]` (`FUN_000cecf0`); from USD: `amount = usd * rate[code]` (`FUN_000cf150`). A missing rate gives NaN, so the line shows no result.
- `rate[code]` is units of that currency per 1 USD (rates JSON, §10).
- Coins from `coins2.json` are registered the same way under category `crypto` [obs]; their ids and phrases come from that file [inf].
- Formats come from `L/Currency.strings`: `USD` `$%@`, `EUR` `€ %@`, `GBP` `£ %@`, `RUB` `%@ ₽`, `BTC` `%@ B⃦`, and so on.

### 1.3 Derived area and volume units [obs: `FUN_00062670`, `FUN_00051d60`]

- The length builder hands a list of length units to the area and volume builders: `m`, every SI-prefixed metre (ym … Ym), `inch`, `foot`, `yard`, `mile`. Point, cable, league, furlong, nautical mile and chain are registered directly and get no square or cube forms.
- For each such unit `u` and power `n` (2 = area, 3 = volume):
  - id = `u.id + "n"` (`m2`, `km3`, `foot2`); baseUnitId = `m2` / `m3`.
  - ratio = `u.ratio ^ n` (decimal power, rounding up at scale 38).
  - phrases: each template in `area.templates` / `volume.templates`, applied with `String(format:)` to every phrase of `u` and to both of its symbols. English templates: area `square %@, sq %@, sq%@, %@², %@ ², %@ 2`; volume `cu %@, cubic %@, cb %@, cb%@, %@³, %@ ³, %@ 3`. So `square meter`, `sq ft`, `km²`, `cubic inch`, `cm 3` and `m3` all parse.
  - format: `area.format` `%@²` / `volume.format` `%@³` applied to `u.format`, giving `m²`, `ft²`, `″²`, `mi.²`, `km³`.

### 1.4 SI and IEC prefixes [obs: tables `FUN_000d1720` (SI), `FUN_000d0580` (IEC); expansion `FUN_00055c10`, `FUN_0005b4e0`]

SI (name, symbol, power of 10): yocto y -24, zepto z -21, atto a -18, femto f -15, pico p -12, nano n -9, micro µ -6, milli m -3, centi c -2, deci d -1, deca da 1, hecto h 2, kilo k 3, mega M 6, giga G 9, tera T 12, peta P 15, exa E 18, zetta Z 21, yotta Y 24. Localized symbols come from `L/SIPrefixes.strings`. The code uses `μ` (U+03BC) for micro and the strings file uses `µ` (U+00B5); accept both.

IEC (name, symbol, power of 1024): kibi Ki 1, mebi Mi 2, gibi Gi 3, tebi Ti 4, pebi Pi 5, exbi Ei 6, zebi Zi 7, yobi Yi 8.

- Each prefix on an `isSiUnit` unit yields a unit with ratio `base.ratio * 10^p` (or `1024^p`), phrases `prefixName + phrase` (`kilometer`, `kilometre`) and `prefixSymbol + symbol` (`km`). The concatenation is [obs]; the exact id string is [inf].
- `FUN_00055c10(unit, allowNegative)`: with the flag off, only prefixes with p ≥ 0 are generated. Data units pass false, so there is no `mb` (millibit). All other SI units pass true.
- SI units: `m`, `litre`, `gram`, `tonne`, `second`, `bit`, `byte`. IEC: `bit`, `byte`.

### 1.5 Constants (context variables) [obs: `FUN_00046370`, `FUN_00046930`]

| name | value |
|---|---|
| `pi`, `π` | `NSDecimalNumber(double: 3.141592653589793)` |
| `e` | 2.718281828459045235360287471352662497757247093699959574966967627724076630353547 (80-digit string) |
| `r` | 8.3144598 (molar gas constant) |
| `ppi` | 96 |
| `em` | 16 px (hidden) |

These are ordinary variables, so a document can shadow them [inf].

---

## 2. Grammar and precedence

### 2.1 Token types (extractors) [obs: registration `FUN_0004c1f0`; patterns from `decoded.tsv`]

| element type | extractor | rule |
|---|---|---|
| `num` | DecimalNumberExtractor `FUN_0009d3a0` | extended-syntax regex `(?:[+-][\s]*)? (\.? \d+ [\s\d\._,]*) (?<!\s\|,\|\.)`. A leading sign belongs to the number. Digits may be grouped with spaces, `_`, `,` or `.`. The token cannot end in a space, comma or dot |
| `num`, `nsystem=hex` | HexNumberExtractor `FUN_0009ebb0` | `0x([0-9a-fA-F]+)` |
| `num`, `nsystem=bin` | BinaryNumberExtractor `FUN_0009c1c0` | `0b([0-1]+)` |
| `num`, `nsystem=oct` | OctalNumberExtractor `FUN_000a51f0` | `0o([0-7]+)` |
| `num`, `nsystem=sci` | ScientificNumberExtractor `FUN_000a91a0` | `\.? \d+ [\s\d\._,]* (?<!\s\|,\|\.) [Ee] [+-]? \d+` |
| `oprtr` | OperatorExtractor `FUN_000a5570` | symbols and phrases from §2.2, matched on word boundaries [inf] |
| `oprtr` (`*`) | MultiplyXSymbolExtractor `FUN_000a0b80` | the letter `x` as multiplication [obs], presumably only between numeric operands so `x` still works as a variable or unit [inf] |
| `unit` (`percent`) | PercentageSymbolExtractor `FUN_000a7570` | `%` |
| `prnth` | ParenthesesExtractor `FUN_000a6cf0` | `[\(\)]` |
| `hdr` | HeaderExtractor `FUN_0009e190` | `\s*\#.*`: optional spaces, then `#`; the whole line is a header |
| `lbl` | LabelExtractor `FUN_0009ef70` | `.*:`. Greedy, so everything up to and including the last colon is a label and takes no part in the calculation. How this interacts with `2:30 pm` is [unk]; the time extractor probably claims those characters first |
| `cmnt` | SlashCommentExtractor `FUN_000a9560` | `//.*` to end of line |
| `cmnt` | QuotedCommentExtractor `FUN_000a7f80` | `"[^"]*"`, anywhere in the line |
| `unit` | UnitExtractor `FUN_000aa850` | longest unit phrase match [inf] |
| `unit` (`scale`) | ScalesExtractor `FUN_000a8660` | §4.3 |
| `var` | VariablesExtractor `FUN_000aaf60` | names of assigned variables (context key `var_<name>`, hidden ones flagged `hidden_var_<name>`) |
| `fn` | FunctionExtractor `FUN_0009d9b0` | phrases from §9 |
| `unit` (`nsystem`) | NumeralRepresentationExtractor `FUN_000a1680` | `hex`, `binary`/`bin`, `octal`, `decimal`/`dec`, `scientific`/`sci`/`exp`/`exponent`/`exponential` (`L/Numbers.strings`), used after a conversion word |
| `unit` (`time_zone`) | TimeZoneAbbreviationsExtractor `FUN_000a9c20`/`FUN_000a9f40` | §8.3 |
| assignment | AssignmentExtractor `FUN_0009a870` | `=` plus `equal`, `is` (`assignment.variants`) |
| times | DateExtractor (`FUN_0009c590`) | uses `hour.variants` etc. for `3pm`, `2:30 pm` [inf] |

Anything no extractor claims is free text and is ignored, so `$9 for coffee` evaluates `$9` [inf; matches documented Numi behaviour]. Unknown words between two values probably break the expression [inf].

### 2.2 Operators [obs: definitions `FUN_0004ea10`; symbol arrays decoded from static data]

| id | symbols | word phrases | priority |
|---|---|---|---|
| `in` (conversion) | – | `in, into, as, to` | 300 |
| `+` | `+` | `with, plus, and` | 100 |
| `-` | `-` ‒ – — ― − (U+2012..2015, U+2212) | `minus, without, subtract` | 100 |
| `*` | `*` `✕` `×`, plus the letter `x` (§2.1) | `times, multiplied by, multiply, mul, mult` | 200 |
| `/` | `/` `÷` | `divide, divide by, divided by` | 200 |
| `^` | `^` `ˆ` (U+02C6) | – | 300 |
| `mod` | – | `mod` | 200 |
| `and` (bitwise AND) | `&` | none; the word `and` means `+` | 200 |
| `or` (bitwise OR) | `\|` | [unk] | 200 |
| `xor` | – | `xor` | 200 |
| `left_shift` | – | `<<` | 200 |
| `right_shift` | – | `>>` | 200 |
| `percent_of` | – | `of, from` | 100 |
| `percent_on` | – | `on` | 100 |
| `percent_off` | – | `off` | 100 |
| `as_a_percent_of` | – | `as a % of, as a percent of` | 100 |
| `as_a_percent_off` | – | `as a % off, as a percent off, as a % decrease of, as a percent decrease of` | 100 |
| `as_a_percent_on` | – | `as a % on, as a percent on, as a % increase of, as a percent increase of` | 100 |
| `percent_of_what_is` | – | `of what is` | 100 |
| `percent_off_what_is` | – | `off what is` | 100 |
| `percent_on_what_is` | – | `on what is` | 100 |

Numi has no postfix `!` [obs: no `!` operator or literal]. Factorial is the function `fact`.

### 2.3 Evaluation order [obs: `FUN_000c6b80`, `FUN_00090e90`]

- The evaluator repeatedly takes the operator with the highest priority, picking the leftmost one on ties (it compares with a strict `>` while scanning left to right), applies it to its neighbours, and repeats.
  - Every binary operator is therefore left-associative, `^` included: `2^3^2 = (2^3)^2 = 64` [obs logic; verify on a real install].
  - `a * b ^ c` = `a * (b^c)` (300 > 200); `a + b * c` = `a + (b*c)`; `5 & 3 + 1` = `5 & 4` (200 > 100). Unlike C, the bitwise operators bind tighter than `+` and `-`.
  - `in`/`to` has priority 300, the same as `^`. By the leftmost rule, `1 + 2 m in cm` converts `2 m` first and then adds. Whether Numi converts the whole line first is [unk]; `ExplicitUnitConversionMatcher` probably treats a trailing `in <unit>` separately.
- Unary minus [obs: `FUN_00090e90` gives a `-` operator with no left operand the attribute `opPriority = 1000`]: it binds tighter than everything else, so `-2^2 = 4` and `2 * -3 = -6`. A `-` written directly before digits is already part of the number token (§2.1).
- Parentheses are grouped and evaluated as sub-expressions [inf: `ParenthesesExtractor` plus an `opPriority` attribute that overrides operator priority when present].
- Implicit multiplication (`2(3)`, `2 pi`, `2 x` with a variable `x`) is [unk]. The binary shows no evidence for it and Numi's docs do not mention it. Default: unsupported (Open questions).

### 2.4 Arithmetic with units (operation implementations, `FUN_0004ff50`) [obs]

Each operator id has several registered implementations, tried in order; the first that accepts the operand types runs [inf: the "priority 100" argument is a registration weight].

- `+`: DateAndDecimalAddition (`FUN_000c45e0`, date + duration; the duration is converted to seconds and rounded), PercentageAddition (`FUN_000ca200`), DecimalAddition (`FUN_000c5880`).
- `-`: DateSubtraction (`FUN_000c5180`, date - date = seconds as a double, unit `second`), DateAndDecimalSubtraction, PercentageSubtraction (`FUN_000ca830`), DecimalSubtraction (`FUN_000c5eb0`).
- `*`: PercentageMultiplication (`FUN_000ca590`), DecimalMultiplication (`FUN_000c5d80`).
- `/`: DecimalDivision (`FUN_000c5ad0`).
- `^`: Exponent (`FUN_000c6240`). An integer exponent uses exact decimal `raisingToPower`; anything else uses `pow(f64, f64)`.
- `mod`: `FUN_000c6650`, computed in f64 [inf: C `fmod` semantics, so the result takes the dividend's sign].
- `&`, `|`, `xor`, `<<`, `>>`: on `integerValue`, truncated to i64 [obs: `FUN_000c64d0`].

Unit rules ([obs] for addition and subtraction, [inf] for the rest):

- `a + b`, `a - b`: `b` is converted to `a`'s unit first; the result keeps `a`'s unit and attributes. `5 m + 3 ft` = `5.9144 m`; `$5 + 3 EUR` gives USD. If only `b` has a unit, the result takes it [inf].
- `a * b`: plain product, unit from whichever operand has one [inf]. Units do not compound (`m*m` does not become `m2`) [inf].
- `a / b`: if both have units of the same category the unit drops out (a ratio); otherwise `a`'s unit stays [inf: `FUN_000c5ad0` checks `unit` after dividing].
- Incompatible units (`5 m + 3 kg`): the conversion fails, no implementation applies, and the line has no result [inf].

---

## 3. Percentages [obs: compute functions as listed; operand order inferred from receiver order]

`p` is the percent value (unit `percent`), `x` the other operand. Results keep `x`'s unit unless the table says otherwise.

| syntax | operation | formula | example |
|---|---|---|---|
| `p% of x`, `p% from x` | PercentOf `FUN_000c9040` | `x * p / 100` | `20% of 10` = 2 |
| `p% on x` | PercentOn `FUN_000c9bc0` | `x + x * p/100` | `5% on 30` = 31.5 |
| `p% off x` | PercentOff `FUN_000c9580` | `x - x * p/100` | `6% off 40` = 37.6 |
| `p% of what is y` | PercentOfWhat `FUN_000c92e0` | `y / (p/100)` | `5% of what is 6` = 120 |
| `p% on what is y` | PercentOnWhat `FUN_000c9eb0` | `y * 100 / (100 + p)` | `5% on what is 6` ≈ 5.71 |
| `p% off what is y` | PercentOffWhat `FUN_000c9870` | `y * 100 / (100 - p)` | `5% off what is 6` ≈ 6.32 |
| `a as a % of b` | AsAPercentOf `FUN_000c26b0` | `a / b * 100`, unit `percent`; `a` is converted to `b`'s unit first | `50 as a % of 100` = 50% |
| `a as a % off b` | AsAPercentOff `FUN_000c29a0` | `(b - a) / b * 100`, percent | `70 as a % off 100` = 30% |
| `a as a % on b` | AsAPercentOn `FUN_000c2d00` | `(a - b) / b * 100`, percent | `130 as a % on 100` = 30% |
| `x + p%` | PercentageAddition `FUN_000ca200` | `x + x * p/100` | `100 + 20%` = 120 |
| `x - p%` | PercentageSubtraction `FUN_000ca830` | `x - x * p/100` | `100 - 20%` = 80 |
| `x * p%` | PercentageMultiplication `FUN_000ca590` | `x * p/100` | `50 * 20%` = 10 |
| `x / p%` | none registered | falls back to DecimalDivision, `x / p` with the percent as a plain number [inf] | [unk] |
| `p%` alone | value with unit `percent` | – | `20%` |

- The sample document (`L/Samples.strings`) has `8 / (45 - 20%)` = 8 / 36 ≈ 0.22.
- Percent is a unit with ratio 1, so conversions only happen inside the `percent` base. Whether `0.2 in %` gives `20%` is [unk].

---

## 4. Numbers

### 4.1 Literals [obs: regexes in §2.1]

- Decimal: `1234`, `1 234`, `1_234`, `1,234.5`, `.5`. The regex accepts `,` `.` `_` and spaces inside a number; the user's locale decides which is the decimal separator (en_US: `,` groups, `.` is decimal) [inf]. Underscores and spaces always group [inf].
- Hex `0xFF` (either case), binary `0b1010`, octal `0o17`; the value is the integer. The result keeps its numeral system (`nsystem`), so `0xFF + 1` prints in hex [inf from how attributes propagate].
- Scientific: `1.5e3`, `2E-4`, `1 000e3`.

### 4.2 Output representation (`in hex` etc.)

- `<expr> in hex | binary | bin | octal | decimal | dec | sci | scientific | exp | exponent | exponential` sets `nsystem` to `hex`, `bin`, `oct`, nothing (decimal), or `sci` [obs: `FUN_000a1680`].
- Rendering [obs: `FUN_000ac360`, `FUN_000ae770`]:
  - hex is `"0x"` followed by the lowercase base-16 digits of `integerValue` (the fraction is truncated). Binary uses `0b`, octal `0o`. For negative values the prefix is glued in front of Swift's signed `String(radix:)` output, giving `0x-ff` [obs]. See Open questions.
  - sci uses `NSNumberFormatter` scientific style with max fraction digits = precision (+1 on macOS < 10.15). en_US: `1.23E5`.

### 4.3 Scales [obs: `FUN_00051370`]

| id | multiplier | phrases (`L/Scales.strings`) |
|---|---|---|
| `thousand` | 10^3 | `thousand, Thousand, thousands, Thousands, th., th, k` |
| `million` | 10^6 | `million, Million, millions, Millions, M` |
| `billion` | 10^9 | `billion, Billion, milliard, Milliard, billions, Billions, milliards, Milliards` |

- Scale phrases are case-sensitive: the strings file lists both cases explicitly, and `M` is million while `m` is metre [inf]. There is no trillion.
- `ScaledExpressionMatcher` applies the scale to the number in front of it; the scaled value can still carry a unit or currency (`$5k`, `5k USD`, `3 million EUR`) [inf].

### 4.4 Currency and unit placement [inf: matcher names]

`DecimalFollowedByUnitMatcher`, `UnitFollowedByDecimalMatcher`, `UnitAroundDecimalMatcher` and `DecimalSurroundByCurrencyEvaluator` let a unit or currency phrase sit before or after the number, with or without a space: `$5`, `5$`, `5 USD`, `USD 5`, `€5`, `5 €`, `5 euro`. The same holds for ordinary units (`5 m`, `5m`).

---

## 5. Document semantics

- Variables [obs: `AssignmentOperationMatcher`, context keys `var_<name>`]: `name = expr`, `name equal expr`, `name is expr`. Values keep their units, and the assignment line shows the assigned value [inf]. Allowed name characters and case sensitivity are [unk]. Default: letters, digits and `_`, starting with a letter, case-sensitive.
- `prev` [obs: context `prev_result`, set after every line in `FUN_000650f0`] is the result of the line directly above. If that line had no result, `prev` is empty and the expression fails [inf].
- `sum`/`total` and `avg`/`average` [obs: `FUN_000b09c0`, `FUN_000b40b0`]: every line appends its result, empty or not, to `prev_results`. `sum` walks backwards from the previous line and collects values until it meets an empty one (a blank line, header, comment-only line, or a line that failed). It then folds the collected values with the normal `+` operation, so unit conversion applies and incompatible units make the sum fail [inf]. `avg` is that sum divided by the count.
  - The fold order decides the result's unit. [inf] It is the unit of the top line of the block; verify.
- Grand total [obs: context `grand_total`, `grand_total_formatted`]: a running `+` over every line result in the document, shown in the bottom bar. Lines whose addition fails are skipped [inf].
- Headers: a line matching `\s*#.*` is a header, styled, with no result [obs]. It ends a `sum` block because it produces an empty value [inf].
- Labels: in `Label: 5` the `Label:` part is ignored and the result is 5 [obs regex].
- Comments: `//` to end of line, and `"quoted text"` anywhere in a line [obs]. A comment-only line has no result.

---

## 6. Conversion rules

- Conversion words: `in`, `into`, `as`, `to` [obs]. The target can be a unit, a currency, `percent` [inf], a numeral system (§4.2), a time zone (§8.3) or a place name (GeoNames).
- `ConversionOperation` only converts within one base unit. Currency goes through USD (§1.2).
- Mixed-unit arithmetic takes the left operand's unit (§2.4).
- Mixed currencies: `$5 + 3 EUR` gives a USD result at the latest rates. With no rates (offline before the first fetch) the converters return NaN and the line has no result [obs: converters return `notANumber`].
- A failed conversion between incompatible categories leaves the line without a result; Numi shows nothing rather than an error message [inf].

---

## 7. Formatting [obs: `FUN_000ae5e0` formatter init, `FUN_000ac360` value formatter, `FUN_000ae770`]

- Two `NSNumberFormatter`s on the user's locale: decimal style (`numberStyle = 1`) and scientific style (`4`), both with `roundingMode = 6`, which is half-up.
- Max fraction digits = context `precision`, 2 when unset [obs: fallback `lVar14 = 2`]. The app's precision preference sets it (see `spec-ui.md`).
- Min fraction digits is 0, except for currencies (units whose base is `USD`): if the number has exactly one fractional digit, the minimum becomes 2. So `$1.5` shows `$1.50`, while `$1.25` and `$1` stay as they are [obs: `(fractionDigitsCount == 1) * 2`].
- Grouping follows the locale's decimal style (en_US `1,234,567.89`) [obs: decimal style groups by default].
- There is no automatic switch to scientific notation [inf]. The decimal formatter is used unless `nsystem = sci`, so tiny numbers show as `0` at 2 decimals and huge numbers print every digit.
- Negative numbers are formatted as their absolute value, then `-` goes in front of the whole unit template: `-$5`, `-5 m` [obs: subtract from zero, then prepend `-`].
- Unit rendering follows §1.1. Percent's format is `%%`, and the rule turns it into `"<n> %%"`, which `String(format:)` collapses to `"20 %"`. Whether Numi actually shows `20%` or `20 %` is [unk]; check on a real install.
- NaN: no result is displayed [inf: the formatter compares with `notANumber` first].
- A formatted value has `regular` and `short` variants (`FormattedValue.regularKey`/`shortKey`) [obs]. The short one is used where space is tight, e.g. dates (§8.4).

---

## 8. Dates and time zones

### 8.1 Values [obs: `FUN_000b4bd0`, `FUN_000b5200`, `FUN_000b54b0`, `FUN_000b5730`]

| phrase (`L/Dates.strings`) | value |
|---|---|
| `today` | `current_time` with unit `date`, not truncated to midnight |
| `tomorrow` | `current_time + 86400 s` |
| `yesterday` | `current_time - 86400 s` |
| `time`, `now` | `current_time` with the local `time_zone` attribute |
| `local`, `local timezone`, `local time zone` | the local time zone (`FUN_000b2c80`, key `current_time_zone`) |

`current_time` is captured once per evaluation pass and stored in the context [obs: key `current_time`], so all lines of one recalculation see the same instant.

### 8.2 Arithmetic [obs]

- `date ± duration`: the duration is converted to seconds with the fixed ratios from §1.2, rounded, and added (`FUN_000c45e0`). `today + 1 month` adds 30 days and `+ 1 year` adds 365 days; there is no calendar arithmetic. The rounding mode and scale used here are [unk]; default to whole seconds, half-up.
- `date - date` gives seconds as a `second` value, computed in f64 (`FUN_000c5180`). Convert with `in days` etc.
- `fromunix(x)` / `unixtime(x)` [obs: phrases, `FUN_000b1fd0`]: date from Unix seconds, read as UTC and shown in local time.
- `gmt` [obs: `FUN_000b24b0`]: `GMT+3` or `gmt -5` gives a fixed-offset zone of `n * 3600` seconds. Fractional hours are truncated to whole seconds [inf].

### 8.3 Time zones

- Abbreviations [obs: `FUN_000a9c20` reads Foundation's `TimeZone.abbreviationDictionary`, skips `GMT`, adds lower-cased phrases]. The table belongs to the OS, not to Numi. Embed a copy of the macOS dictionary [inf: from Foundation, verify on macOS]: ADT, AKDT, AKST, ART, AST, BDT, BRST, BRT, BST, CAT, CDT, CEST, CET, CLST, CLT, COT, CST, EAT, EDT, EEST, EET, EST, GMT, GST, HKT, HST, ICT, IRST, IST, JST, KST, MDT, MSD, MSK, MST, NZDT, NZST, PDT, PET, PHT, PKT, PST, SGT, TRT, UTC, WAT, WEST, WET, WIT, each mapped to an IANA id (`CET→Europe/Paris`, `EST→America/New_York`, `PST→America/Los_Angeles`, `HKT→Asia/Hong_Kong`, `IST→Asia/Kolkata`, `JST→Asia/Tokyo`, `BST→Europe/London`, `MSK→Europe/Moscow`, …). IST, CST and BST are ambiguous; follow the macOS mapping.
- A time with a zone (`2:30 pm HKT`) goes through `TimePrefixedByTimeZoneMatcher`; `… in Berlin` or `in EST` converts through `TimeZoneConversionOperation`.
- City and place names [obs: `GeoNamesService.timeZoneIDForPlace`, `GeoNameSequenceMatcher`, `postCalculationHandleGeoNames`]: unknown words in a time context are sent to `https://s1.numi.app/timezone?q=<place>` (query parameter `q`), and the response yields an IANA zone id [inf: plain id or JSON, not seen]. Results are cached and the line is recalculated when the async reply arrives [inf]. HTTP 503 is retried up to `maxRetryCount = 4` with a "retry in …" delay [obs]. Offline, only abbreviations, `GMT±n` and `local` work. For Numero, ship an offline city→IANA table (e.g. GeoNames cities15000) instead of calling a server.

### 8.4 Output format [obs: `FUN_000ab780` creates three `DateFormatter`s on the user's locale; selection in `FUN_000aba90`]

| formatter | dateStyle | timeStyle | en_US example |
|---|---|---|---|
| A | short | short | `10/14/26, 3:45 PM` |
| B | none | short | `3:45 PM` |
| C | short | none | `10/14/26` |

Selection [inf from the branch structure, which calls two calendar predicates `FUN_00095c10`/`FUN_00095d40`]: a value whose time falls on the current day uses B (`now`, `time`, `2:30 pm HKT in Berlin`); any other day uses A for `regular` and C for `short`. `today` and `tomorrow` carry the current clock time, so they would show a time unless one predicate tests "time is midnight" [unk]. Check what `today` prints on a real install.

---

## 9. Functions [obs: registration `FUN_0004db90`, phrase arrays decoded; implementations `FUN_000af…`–`FUN_000b5…`]

Call syntax is `name(x)` or `name x`. Multi-argument functions take a sequence of values; `Contents/Resources/Sample.js` separates them with `;` (`zum(2;3)`). The engine has no separator literal [obs], so `;` is free text that happens to split the numbers [inf]. `log 2 (8)` reads the value before the parenthesis as the first argument [inf].

| function | phrases | args | behaviour |
|---|---|---|---|
| sqrt | `sqrt, square root` | 1 | √x (f64) |
| cbrt | `cbrt, cubic root, cube root, cubed root` | 1 | `cbrt` (f64) |
| root | `root` | 2 | `root n (x)` = x^(1/n); for x < 0 and odd n it returns `-((-x)^(1/n))` [obs `FUN_000b3430`] |
| abs | `abs` | 1 | `|x|` in decimal (`0 - x` when negative) |
| log | `log` | 2 | `log b (x)` = log2(x) / log2(b); base 1 gives NaN [obs `FUN_000b3140`]. One-argument `log x` is [unk]; default base 10 |
| ln | `ln` | 1 | natural log; `ln 0` gives NaN [obs `FUN_000b2a40`] |
| fact | `fact` | 1 | `tgamma(x + 1)`, so non-integers work (`fact 0.5` ≈ 0.886) [obs `FUN_000b1c80`] |
| sin, cos, tan | `sin`, `cos`, `tan` | 1 | a unitless argument is in degrees; `rad`/`°` units are honoured; the result is unitless [obs `FUN_000b16c0` etc.: no unit → `degree` → converted to radian] |
| arcsin, arccos, arctan | `arcsin`, `arccos`, `arctan` | 1 | result in degrees (computed in radians, then converted) [obs `FUN_000afda0`] |
| sinh, cosh, tanh | `sinh`, `cosh`, `tanh` | 1 | plain f64, no unit handling |
| round | `round` | 1 | to an integer with NSRoundPlain, i.e. half away from zero [obs `FUN_000b37b0`: scale 0, mode 0] |
| ceil, floor | `ceil`, `floor` | 1 | f64 |
| sum, avg, prev | §5 | 0 | |
| today, tomorrow, yesterday, time/now, local | §8 | 0 | |
| fromunix | `fromunix, unixtime` | 1 | §8.2 |
| gmt | `gmt` | 1 | §8.2 |

- There is no `pow()` or `exp()` function (use `^` and `e^x`) and `%` is never modulo [obs: not registered].
- Constants: §1.5.
- User extensions (JavaScriptCore, `JSNumi`): `numi.setVariable(name, {double, unitId} | number)`, `numi.addUnit({id, phrases, baseUnitId, format, ratio, internationalSymbol})`, `numi.addFunction({id, phrases}, fn(values) → {double, unitId?})` (`Contents/Resources/Sample.js`). Values cross that boundary as `{double, unitId}` (f64). For Numero this is optional, maybe a later scripting hook.

---

## 10. Exchange rates [obs]

- Endpoints (`Calculator.Config`, `FUN_00038830`/`FUN_000388d0`): `https://s3.numi.app/rates/currency.json` and `https://s3.numi.app/rates/coins2.json`. Numero should not call these; pick another source (ECB, exchangerate.host, Coinbase or similar).
- `RatesJson` CodingKeys: `rates` (code → number) and `timestamp` (`FUN_0003a5a0`). `CurrencyRatesJson` decodes `rates` [obs `FUN_00039760`]. Rates are units per 1 USD (§1.2). `coins2.json` probably uses the same `rates` map [inf].
- Refresh: `Calculator.Config.currencyRefreshRate` defaults to 21600 s, i.e. 6 hours (`DAT_00109218`). A `PeriodicFileLoader` fetches each URL at that interval. The last payload is cached under `currency_rates` / `crypto_rates`, with timestamps `currency_rates_update_time` / `crypto_rates_update_time` [obs: `FUN_00048990`, `FUN_000496b0`], and the cache is used at startup before the first fetch [inf].

---

## 11. UI behaviour

The detailed UI spec is `docs/numi-spec-ui.md` (preferences window, font-size tables, theme hex colours for every token class, result column hover/copy, list window, menu-bar mode, main menu). The feature-by-feature parity checklist is §12 below. Engine-side facts the UI relies on:

- Element type tags for syntax colouring [obs]: `num`, `oprtr`, `oprtrsmb`, `unit`, `prnth`, `var`, `fn`, `cmnt`, `hdr`, `lbl`, `time_zone`. `CalculatorTheme` maps them to colours (spec-ui §3).
- `grand_total_formatted` feeds the bottom total bar.
- The UI's precision preference (default 2, range 0-10) sets the engine's context `precision` (§7).

---

## 12. Feature inventory for parity

Goal: full parity with Numi 3.35 except the Alfred integration. One line per feature. §12.1 covers the engine and points back to the sections above for detail. §12.2 onwards covers the app; it was compiled in a second pass from `Numi.x86_64` (Ghidra dump in `/home/psoldunov/re/numi/dump-app/`, helpers in `/home/psoldunov/re/numi/scripts2/`), the nibs and the plists. In those tables `APP@0x1000…` is a function in `thin/Numi.x86_64` and `UI@0x…` one in `thin/NumiUI.x86_64`. Scope marks: [parity] build it, [out?] store, licensing or telemetry (decide), [skip] not for the clone.

### 12.1 Engine

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Decimal arithmetic | 38 significant digits, NaN instead of errors | §0 | obs |
| Units: length, area, volume, weight, time, data, angle, temperature, CSS | tables and ratios | §1.2 | obs |
| Square/cube units from templates | `square %@`, `%@²`, `cubic %@`… on m, SI metres, inch, foot, yard, mile | §1.3 | obs |
| SI and IEC prefixes | 20 SI, 8 IEC; data units take positive SI only | §1.4 | obs |
| Currencies (≈160 ISO + BTC) and crypto coins | through USD, rates per USD | §1.2, §10 | obs |
| Rates refresh | every 6 h, cached with update time | §10 | obs |
| Constants `pi`/`π`, `e`, `r`, `ppi`, `em` | | §1.5 | obs |
| Number literals: decimal with grouping, hex, bin, oct, sci | | §4.1 | obs |
| Output in hex/bin/oct/dec/sci | `in hex` etc. | §4.2 | obs |
| Scales k/thousand, M/million, billion/milliard | | §4.3 | obs |
| Unit/currency before or after the number | `$5`, `5$`, `5 USD` | §4.4 | inf |
| Operators + − × ÷ ^ mod & \| xor << >> with word forms | priorities 100/200/300, leftmost first | §2.2-2.3 | obs |
| Unary minus binding tighter than `^` | | §2.3 | obs |
| Parentheses | | §2.3 | inf |
| Conversion words in/into/as/to | | §6 | obs |
| Mixed-unit arithmetic (left unit wins) | | §2.4 | obs |
| 12 percent operations | of/on/off, …what is, as a % of/on/off, x ± p%, x × p% | §3 | obs |
| Variables with `=`, `equal`, `is` | | §5 | obs |
| `prev`, `sum`/`total`, `avg`/`average` | block ends at an empty line | §5 | obs |
| Grand total bar | | §5 | obs |
| Headers `#`, labels `…:`, comments `//` and `"…"` | | §2.1, §5 | obs |
| Free text ignored | | §2.1 | inf |
| Functions sqrt, cbrt, root, abs, log, ln, fact, trig (degrees), arc-trig, hyperbolic, round, ceil, floor | | §9 | obs |
| Dates today/tomorrow/yesterday/now/time, date arithmetic | fixed 30-day month, 365-day year | §8.1-8.2 | obs |
| `fromunix`/`unixtime`, `GMT±n`, `local` | | §8.2 | obs |
| Time zone abbreviations and conversion | macOS abbreviation table | §8.3 | obs |
| City names (`Berlin time`, `time in New York`) | online GeoNames lookup in Numi; offline table for Numero | §8.3 | obs |
| Date output: short date/time styles on user locale | | §8.4 | obs/inf |
| Formatting: precision, half-up, grouping, currency 2-decimal rule, negatives | | §7 | obs |
| Localized vocabulary (27 engine locales, English always merged in) | | §1.1 | obs |
| JS extensions: `numi.setVariable`, `numi.addUnit`, `numi.addFunction`, `log()` | loaded from `<App Support>/<bundle id>/extensions/**/*.js` at startup | §9, §12.8 | obs |
| QuickType prefix hints | engine API only, no UI in 3.35 | §12.11 | obs |

---

### 12.2 Preferences and defaults (spec-ui §1 has the full table and the per-key notes)

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Precision | Int, default 2, field accepts 0..10, integers only | `APP@0x100007470`; nib `GeneralPreferences.nib` obj 106 | obs |
| Font size | Int 0..3 = Dynamic/Small/Medium/Large, default 0 (Dynamic) | `APP@0x100007470`, `UI@0x2e940` | obs |
| Night mode | Bool, default **true** | `APP@0x100007470` | obs |
| Always on top | Bool, default false; level = status window (9), else normal (4) | `APP@0x10002d860` | obs |
| Show in menu bar | Bool `menuBarMode`, default false | `APP@0x100007470` | obs |
| Launch at login | Not a default; `SMLoginItemSetEnabled` for `com.dmitrynikolaev.numi3helper`, state read from helper | `APP@0x100004fbd` | obs |
| Global hotkey | `shortcutObject` dict, **no default** (re-checked, see §3) | `APP@0x100027970`, `0x100008580` | obs |
| Alfred integration | Bool, default false; Swifter HTTP server in `Swifter.framework` | `APP@0x100007470` | obs, [skip] |
| Window frame | `windowFrame` Data (archived `NSValue` rect), default 370x360 centred | `APP@0x100007470` | obs |
| User window frame | `userWindowFrame`, same default; role unknown | same | obs / unk role |
| Result pane width | `resultPaneWidth` Int, default 120 px | same | obs |
| Has launched before | `hasLaunchedBefore` Bool false; set true on first run | `APP@0x10002c870` | obs |
| Clean-exit flag | `correctFinish` Bool, default true (hidden, "Rough termination" path) | `APP@0x10002a5e0` | inf |
| Engine cache disabled | `isEngineCacheDisabled` Bool false (hidden) | `APP@0x100007470` | obs |
| Activation on update | `activationOnUpdate` Bool false (licence flow) | `APP@0x100007470` | obs, [out?] |
| What's new version | `whatsNewVersion` string/int, no default | key enum `APP@0x100007270` | obs |
| Sparkle keys | `SUEnableAutomaticChecks`, `SUAutomaticallyUpdate`, `SULastCheckTime`, `SUSkippedVersion`, `SUUpdaterIsInDevMode` | decoded strings `APP@0x1000321d0`, `0x100030cd0`, `0x10000fbe0` | obs, [out?] |
| Crash flag | `NSApplicationCrashOnExceptions=YES` registered | `APP@0x1000130d0` | obs, [skip] |
| Preferences tabs | "General", "Extensions" (JS docs text plus "Open extensions folder" button) | `Localizable.strings` | obs |
| Feedback button | `mailto:mail@numi.app?subject=Numi%20feedback%20(<build>)` | `APP@0x100032880` | obs, [out?] |
| Thousands/decimal separator pref | none exists; locale-driven | no key in enum | obs (negative) |
| Currency refresh pref | none | no key | obs (negative) |
| Languages | 30 UI languages: af ar bs-BA cs da de de-AT de-CH es fa fi fr he it ja ko nl nl-BE no pl pt-BR pt-PT ro ru sr sv tr uk zh-Hans zh-Hant (+ Base/en) | `Resources/*.lproj` | obs |
| Localized files per language | `ActivationWindow`, `GeneralPreferences`, `Localizable`, `MainMenu`, `MoveApplication` .strings | `Resources/de.lproj` | obs |
| Move-to-Applications prompt | `MoveApplication.strings` (PFMoveApplication) | `Resources/Base.lproj/MoveApplication.strings` | obs, [skip] |

### 12.3 Menus (nib order; spec-ui §6.3 has the modifier decoding)

Top-level order: Numi, File, Edit, Window, Help, Demo. Cmd = ⌘, Opt = ⌥, Shift = ⇧.

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Numi menu | About Numi (standard panel) · — · Preferences… `Cmd+,` (`preferences:`) · — · Services · — · Hide Numi `Cmd+H` · Hide Others `Cmd+Opt+H` · Show All · — · Check for Updates… (`checkForUpdates:`) · Send Feedback (`sendFeedback:`) · Deactivate (`deactivate:`) · — · Quit Numi `Cmd+Q` (`quitFromMenu:`) | `MainMenu.nib` obj 468 | obs |
| File menu | New `Cmd+N` (`onNewMenuItem:`) · Import `Cmd+O` (`importFile:`) · Export `Cmd+S` (`exportToFile:`) · — · List `Cmd+L` (`onListMenuItem:`) · — · Print `Cmd+P` (`print:`) · — · Close `Cmd+W` (`closeWindow:`) | obj 491 | obs |
| Edit menu | Undo `Cmd+Z` · Redo `Cmd+Shift+Z` · — · Cut · Copy · **Copy Result `Cmd+Shift+C`** (`copyResult:`) · **Copy All `Cmd+Opt+C`** (`copyAll:`) · Paste · Delete · Select All · **Delete All `Cmd+Opt+Backspace`** (`deleteAll:`) · **Surround with Parentheses `Cmd+Shift+0`** (`surroundWithParentheses:`) · — · Find ▸ · Spelling and Grammar ▸ · Substitutions ▸ · Transformations ▸ · Speech ▸ | obj 502 | obs |
| Find submenu | Find… `Cmd+F` · Find and Replace… `Cmd+Opt+F` · Find Next `Cmd+G` · Find Previous `Cmd+Shift+G` · Use Selection for Find `Cmd+E` · Jump to Selection `Cmd+J` (all `performFindPanelAction:` except Jump = `centerSelectionInVisibleArea:`) | obj 512 | obs |
| Spelling submenu | Show Spelling and Grammar `Cmd+:` · Check Document Now `Cmd+;` · — · Check Spelling While Typing · Check Grammar With Spelling · Correct Spelling Automatically | obj 520 | obs |
| Substitutions submenu | Show Substitutions · — · Smart Copy/Paste · Smart Dashes · Smart Links · Data Detectors · Text Replacement | obj 530 | obs |
| Transformations submenu | Make Upper Case · Make Lower Case · Capitalize | obj 539 | obs |
| Speech submenu | Start Speaking · Stop Speaking | obj 547 | obs |
| Window menu | Minimize `Cmd+M` · Zoom · — · Bring All to Front | obj 555 | obs |
| Help menu | Docs `Cmd+?` (`docs:`; opens `https://numi.app/docs/`) | obj 565 | obs |
| Demo menu (visible) | Welcome/General/Conversion/Sum/Dates/Variables/Functions/CSS Sample · — · Play Site Demo `Cmd+Opt+P` · — · Play General/CSS/Dates Demo; "Enabling demo" log path in `APP@0x10002a5e0` | obj 42, `APP@0x100013ec0..0x100014070` | obs, [skip] |
| Status-item menu ("MenuBar Menu") | Import · Export · Print · Copy to Clipboard · — · Docs · Preferences · Check for Updates · — · Activation item (title "Activation", `APP@0x10000ee90`) · — · Quit | `APP@0x10000e600` | obs |
| Export button menu ("Export Menu") | Export · Copy to Clipboard · Print | `APP@0x10000e280` | obs |
| Edit menu uses standard responder actions | Undo/Redo via `NSUndoManager`; spelling/substitution toggles act on the text view | `MainMenu.nib` connectors | obs |
| Touch Bar | none (`NSTouchBar` absent from both binaries) | grep of strings | obs (negative) |
| Services provider | none (`NSServices` absent in Info.plist; only the standard Services submenu) | `plist: Info.plist` | obs |

### 12.4 Global hotkey

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Default hotkey | **None.** `shortcutObject` is not in either `registerDefaults:` call, and no code writes it other than the recorder delegate | `APP@0x100007470`, `0x1000130d0`, whole-dump search for `shortcutWith…` (only `shortcutWithDictionary:`) | obs |
| Recorder config | `SRRecorderControl`, allowed modifiers `0x1e0000` (Ctrl Opt Shift Cmd), none required | `APP@0x10000d030` | obs |
| Storage | `NSUserDefaults["shortcutObject"]` = `SRShortcut.dictionaryRepresentation` | `APP@0x100027970` | obs |
| Re-registration | `SRGlobalShortcutMonitor`: `removeAllActions`, then `addAction` for key-down; runs at launch and after each change | `APP@0x10002eb80` | obs |
| Toggle behaviour | Window visible **and** main → `orderOut` and re-activate the previously frontmost app; otherwise `setCollectionBehavior(moveToActiveSpace=2)`, `makeKeyAndOrderFront`, `activate(ignoringOtherApps: true)` | `APP@0x10002fb60` | obs |
| Hotkey in menu-bar mode | Same function as the status-item click callback (`menuBarControllerStatusChanged:`) | `APP@0x100031680` → `0x10002fb60` | inf |

### 12.5 Menu-bar mode and window behaviour (spec-ui §6 has the base detail)

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| `LSUIElement=true` | Bundle starts as an agent app (no dock icon); the code then sets the policy itself | `plist: Info.plist` | obs |
| Policy: normal mode | `mode=0` → `NSApp.setActivationPolicy(.regular)` (dock icon + app menu) | `APP@0x10002dee0` | obs |
| Policy: menu-bar mode | `mode=1` → `.accessory`; switch is animated and delayed; leaving restores focus through the `com.apple.dock` running app | `APP@0x10002dee0` | obs |
| Status item | `NSStatusItem` + `StatusItemButton`; left click shows/toggles window (`handler(true)`), right click pops up the menu (`handler(false)`) | `UI@0x23d0`, `0x2475` | obs |
| Quit on window close | `applicationShouldTerminateAfterLastWindowClosed` returns false; the app keeps running | `APP@0x1000138f0` | obs |
| Dock click / reopen | `applicationShouldHandleReopen` shows the main window, activates the app | `APP@0x100013900` → `0x100014340` | obs |
| Resign active | Records `activeTime` (timestamp) on `applicationDidResignActive`; use unknown | `APP@0x1000137b0` | obs / unk use |
| Windows | One main window (`MainWindowController.window`); list, preferences and activation are separate auxiliary windows. No multi-window document model | symbols of `MainWindowController`, `AppController` | inf |
| Window restore | `windowFrame` / `userWindowFrame` persisted; default centred 370x360 | spec-ui §1.2 | obs |
| Always on top | Applies to the main and preferences windows | `APP@0x10002d860` | obs |
| Full screen | `window:willUseFullScreenPresentationOptions:` overridden; options unknown | `MainWindowController` symbols | obs / unk options |
| Occlusion | `windowDidChangeOcclusionState` handler exists; effect unknown | `APP@0x100012950` | unk |
| Autosave | `autosaveTimerDing` timer calls the save routine; also saves before New/List | `APP@0x100031700`, `0x10002f140` | obs |
| Termination | `correctFinish` bookkeeping, window frame and pane width saved in `applicationWillTerminate` | `APP@0x100013700`, `0x10002eec0` | inf |
| Launch args | `fresh` (delete all files and settings, message "Fresh start. Delete all files and settings"), `noshadow`, demo enable | `APP@0x10002a5e0`, `0x1000323a0` | obs, [skip] |

### 12.6 Calculation list, export, print (spec-ui §5 has the UI; semantics below)

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| New calculation | Saves the current one, creates an empty storage item, shows the window with empty text | `APP@0x10002a290` | obs |
| Open list | Saves, then shows `NumiListWindowController` anchored to the list button | `APP@0x10002a3d0`, `0x100012550` | obs |
| Select | Delegate `itemSelected(at:)` loads that document into the editor | `UI` symbol `FilterableListViewDelegate` | obs |
| Delete one | Hover delete button → `itemDeleted(at:)`; file removed from storage; no confirmation dialog found | `NumiListCellView.deleteButton` | inf |
| Delete All (Edit) | `CalculatorView.deleteAll:` clears the current document's text only | `UI@0x2cb50` | inf |
| Rename | No rename UI; the label comes from the file name or first line | no evidence in symbols | unk |
| Duplicate | none found | symbol search | unk (probably absent) |
| Drag-reorder | none found; `moveItem(_:to:)` is a model update | `FilterableListView` symbols | inf |
| Filter | `FilterView` text field with `filterText`; applied on the app side (the UI module has no `contains`/`lowercased` in its filter code); case and substring rules unknown | `UI` `FilterView::controlTextDidChange:` | unk semantics |
| Sort order | Storage presenter exposes `entriesSorted`; key unknown (modification time or name) | `APP` strings `entriesSorted` | unk |
| Empty list text | "All your calculations will appear here"; search placeholder "Search" | `APP@0x10000d530` | obs |
| Copy All | `String.stringForPasteboard(selectedRange:results:)`: selected lines with their results, newline-joined; delimiter between text and result unknown | `UI@0x192f0` | obs / unk delimiter |
| Copy Result | Result text of the caret's line | `UI@0x30d50` | obs |
| Export | `NSSavePanel`, allowed type `txt`; writes document text | `APP@0x10000e080`, `0x100032bb0` | obs / inf contents |
| Import | `NSOpenPanel`, allowed type `txt`; menu action `importFile:` | `APP@0x10000e150`, `0x1000306d0` | obs |
| Copy to Clipboard (export menu) | `copyToClipboard:` | `APP@0x1000303e0` | obs / unk format |
| Print | `NSPrintOperation(for: view)`, print theme (all black) | `UI@0x2cf70`, `0x247d0` | obs |
| Open from Finder | `.numi` and `.txt` read as UTF-8 into the editor | `APP@0x100014420` | obs |
| First-launch content | Welcome sample if no documents (`sample.welcome`); `sample.general` text exists | `Localizable.strings` | obs |
| Document persistence | One file per calculation, `NSMetadataQuery` on `%K LIKE '*.txt'`; local fallback in Application Support | `APP@0x100016ec0`, `0x100015c60` | inf |

### 12.7 URL scheme and document types

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| `numi://` registered | `CFBundleURLTypes` name "Numi URL", scheme `numi` | `plist: Info.plist` | obs |
| `numi://` handler | **None found.** No `NSAppleEventManager` use (0 hits), no `application:openURLs:` implementation, no `numi://` string or host/query keys in either binary. The scheme only launches or activates the app | strings of both dumps; `AppDelegate` symbols | inf |
| Document type | `.numi` "Numi Calculations", role Editor, not a package, icon AppIcon | `plist: CFBundleDocumentTypes` | obs |
| UTI declarations | no `UTExportedTypeDeclarations`/`UTImportedTypeDeclarations` | `plist` key list | obs |
| Open file entry | `application(_:openFile:)` → `APP@0x100014420`; accepts `.numi` (and the open panel accepts `.txt`) | `APP@0x100013950` | obs |
| Text type | `.numi` is read as UTF-8 plain text | `APP@0x100014420` | inf |
| Remote endpoints (not read) | `s1.numi.app/{config,announce,timezone,update}`, `s3.numi.app/rates/{currency,coins2}.json`, `numi.app/{docs,buy}` | `APP@0x10001c000`, `0x10001d430` | obs |
| Alfred / HTTP | `Swifter.framework` local server for Alfred | brief + Frameworks | obs, [skip] |

### 12.8 JavaScript extensions (engine side, `NumiEngine.JSNumi`)

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Extensions directory | `<Application Support>/<bundle id>/extensions` (`NSApplicationSupportDirectory`, user domain, plus bundle ID; sandbox container env var `APP_SANDBOX_CONTAINER_ID` is honoured) created on launch | `APP@0x10000f5a0`, `0x10001f920` | obs |
| Seed file | If `extensions/Default.js` is missing, bundle `Sample.js` is copied to it | `APP@0x10002bbf0` | obs |
| Open folder | "Open extensions folder" button in the Extensions tab | `Localizable.strings` | obs |
| Load rule | Enumerates the folder (`FileManager.enumerator(at:)`, so nested folders too), every file with extension `js` is read and `evaluateScript`ed in one shared `JSContext` | `UI@0x42c10` | obs |
| When loaded | At worker build only (`buildNumiWorker(config:mainThread:extensionsUrl:)`); `Sample.js` header says "Please restart Numi on changes". `AppController.rebuildWorker` also rebuilds the worker | `Sample.js`, `UI@0x42c10`, `APP@0x10002b820` | obs |
| Log | Lines "Extension path: …" and "Loading <file>" go to `extensionsLog` | `UI@0x42c10` | obs |
| Errors | JS exception handler logs "Error on line N: message" (uses the exception's `line`) | `UI@0xb6e00` | obs |
| Globals | `numi` (a `JSNumi` object) and a global `log(...)` function | `UI@0xb6b30` | obs |
| `numi.setVariable(name, value)` | Defines a global variable; value is a number or a value dict `{double|decimal, unit?, unitId?}` | `UI@0xb5a40`, `Sample.js` | obs |
| `numi.addUnit(dict)` | Dict keys: `id`, `phrases` (comma list), `baseUnitId`, `format`, `ratio`, `internationalSymbol` | `UI@0xb7cc0`, `Sample.js` | obs |
| `numi.addFunction(dict, fn)` | Dict keys `id`, `phrases`; `fn(values)` receives value dicts, returns `{double: …}`; call syntax `zum(2;3)` | `UI@0xb5c50`, `0xb7370`, `Sample.js` | obs |
| JS value dict | keys `double`, `decimal`, `unit`, `unitId`, plus a `type` key read on results | `UI@0xb7620`, `0xb84d0`, `0xb8ee0` | obs |
| Other JSNumi members | none: `JSNumi` instance methods are only `setVariable::`, `addUnit:`, `addFunction::` (plus init/destruct) | `UI` symbols `JSNumi::*` | obs |
| Disabled cache flag | `isEngineCacheDisabled` default, rebuild path | `APP@0x100007470` | obs |

### 12.9 iCloud and sync

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Entitlements | iCloud container `iCloud.com.dmitrynikolaev.numi`, service `CloudDocuments`, ubiquity container and KV-store identifiers, `disable-library-validation`. **Not sandboxed** (no `app-sandbox`) | `ent:` in `Numi.x86_64` | obs |
| Tracker | `StringStorageFileTracker` + `StringStoragePresenter` (`NSFilePresenter`) + `NSFileCoordinator`; callbacks `fileAdded/Changed/Deleted(url:updateTime:)`, `containerDidChange` | strings `APP@0x1000c93f8..0x1000c9934` | obs |
| Query | `NSMetadataQuery` with `NSMetadataQueryUbiquitousDataScope`; predicate `%K LIKE '*.txt'` | `APP@0x100016ec0` | obs |
| Local-only mode | `start(localOnly:)`; `setUpLocal`; Application Support dir created if missing ("Could not create Application Support directory") | `APP@0x1000ca591` | obs |
| Account change | `ubiquityIdentityDidChange` → rebuilds the tracker | `APP@0x100015630` | obs |
| Download | "Started to download item at …" (starts download of ubiquitous items) | `APP@0x100017e40` | obs |
| Conflict handling | none seen | symbol search | unk |
| KV store | entitlement present; usage unknown | `ent:` | unk |
| Local folder name / file names | unknown | | unk |

### 12.10 Extension and login helper

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| NumiExtension | **Today (Notification Center) widget**, `com.apple.widget-extension`, principal class `NumiExtension.TodayViewController`, display name "Numi", sandboxed, network client, read-only user-selected files, empty app-group list | `PlugIns/NumiExtension.appex/Contents/Info.plist`, `ent:` | obs |
| Widget UI | Hosts `NumiUI.CalculatorView` with `extensionTheme`; has a "Clear Numi widget button" string; talks to `NumiEngine` | strings of `MacOS/NumiExtension` | obs |
| Widget data sharing | No shared app group; widget keeps its own state | `ent:` | inf |
| NumiHelper | Login item `com.dmitrynikolaev.numi3helper` v0.7.1, `LSBackgroundOnly`; on launch it starts `Numi` if not running (`launchApplication:`), then `terminate:`s itself | `Library/LoginItems/NumiHelper.app`, its strings | obs |
| Scope | Widget is deprecated by macOS; helper is the standard login-item pattern | | [out?] / [parity] for login |

### 12.11 QuickType

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Engine | `QuickTypeEngine`: `hints[String] → [QuickTypeHint(string, weight: Int)]`; `register(hint:)`, `register(hints:)`, `hintsFor(text:additionalTokens:weight:)`, `hintsFor(context:additionalHints:)`; `Context.ignoreRegisteredHints` | `UI@0xcb7a0..0xcbcc0` | obs |
| Data | `QuickTypeData { tokens: [String], expressions: [String] }` built by `buildQuickType(mainThread:callback:)`; `QuickTypeExpressionEvaluator` | `UI@0x42550`, `0x38aa0` | obs |
| Switch | `Calculator.Config.enableQuickTypeHints: Bool` | `UI@0x636a0` | obs |
| UI hook | **No UI wiring found.** No NSTextView completion override in `CalcTextView`, and `quicktype` does not appear in the main app. The feature looks like engine API for the widget/HTTP/Alfred paths or unfinished | greps of both dumps | inf |
| Weights / trigger / accept key | unknown | | unk |

### 12.12 Licensing, updates, onboarding, system integration

| Feature | Behaviour / default | Evidence | Conf |
|---|---|---|---|
| Trial / licence | DevMate/Paddle-style trial and activation (`DevMateKit.framework`, `Paddle.framework`, `ActivationManager`, "Trial. %i days left", "Trial is over. Buy now!", Activate/Deactivate menu, `ActivationWindow.nib`) | strings, `Frameworks/` | obs, [out?] |
| Setapp | Public key `Resources/setappPublicKey.pem`, bundle id `com.nikolaeu.numi-setapp` in the code | `APP@0x100005380` | obs, [out?] |
| Sparkle updates | `SUFeedURL=https://s1.numi.app/update`, automatic checks and downloads allowed; menu "Check for Updates…"; dev feed `?feed=dev`; "Numi Updated to %@" user notification with "Read more" | `plist`, `APP@0x10001f790`, `0x10002cd60` | obs, [out?] |
| What's new | Toolbar button tooltip "What's new"; opens `…whats_new?v=` page | `APP@0x100011410`, `0x10002cd60` | obs, [out?] |
| Announcements | `s1.numi.app/announce` fetched, shown via notifications | `APP@0x10001c000` | obs, [out?] |
| Remote config | `s1.numi.app/config`, notification `configUpdated` | `APP@0x10001d430` | obs, [out?] |
| Crash reporting | Sentry (`Sentry.framework`, DSN string), `SentryCrashExceptionApplication` principal class | `plist`, `APP@0x100013580` | obs, [skip] |
| Onboarding | First-launch welcome sample document; demo menu samples | spec-ui §5.3 | obs |
| Services menu | Standard (empty) Services submenu only | nib | obs |
| Drag and drop | none found (`registerForDraggedTypes` absent; only text view default drops) | greps | obs (negative) |
| Accessibility | none custom (`NSAccessibility` absent) | greps | obs (negative) |
| Spell check / substitutions | Standard text-view toggles in the Edit menu | nib | obs |
| Speech | Start/Stop Speaking on the Edit menu | nib | obs |
| Undo/redo | `NSUndoManager`; `setSelectedRangeWithUndo:` customised in `CalcTextView` | `UI@0x8cb0` | obs |
| Text view details | `insertNewline:`, `surroundWithParentheses:`, custom caret colour, pasteboard types incl. `io.numi.unitvalue` | `UI@0x8b40`, `0x9170`, `0xa700` | obs |
| Printing | See §5 | | obs |
| Notifications | `NSUserNotification` (update/what's-new); `didActivateNotification` opens the page | `APP@0x100031a50` | obs, [out?] |
| Geo/time zone | `s1.numi.app/timezone` + GeoNames manager in engine (city time lookups) | `NumiEngine/GeoNamesManager.swift` path in strings | obs, [engine spec] |
| Currency/crypto rates | `s3.numi.app/rates/currency.json`, `coins2.json`; periodic loader with cache | `NumiEngine/*RatesLoader.swift`, `PeriodicFileLoader.swift` | obs, [engine spec] |

### 12.13 Unresolved app-side items

- Document folder name, file naming, and sort key of the list.
- Filter matching rules (case, substring vs prefix).
- Copy All delimiter, Export/Copy-to-Clipboard exact content.
- Whether `numi://` does anything beyond launching the app.
- QuickType trigger/accept key and weights.
- Occlusion handler effect, full-screen options.
- `Assets.car` colour sets and icons (no macOS tooling).

---

## Open questions

1. Exactness or fidelity? Numi's acre (4046.86), nautical mile (1853.184), point (0.00035277777), month (30 d) and year (365 d) differ from the standard values. Options: (a) copy Numi exactly, (b) use the standard values, (c) copy Numi and add the standard units under other names. Default taken: (a), for result parity.
2. Unwired vocabulary (`cbinch` = `cu inch, cui, CI, c.i.`; `cbcm` = `cc, ccm`): (a) ignore them as Numi does, (b) map them to `inch3`/`cm3`. Default: (b). It breaks nothing and matches what the strings file intends.
3. Associativity of `^`: the binary evaluates leftmost first (`2^3^2 = 64`). (a) copy that, (b) make `^` right-associative (512). Default: (a), pending one test on a real install.
4. Implicit multiplication (`2(3)`, `2 pi`): (a) unsupported, as Numi appears to be, (b) support it. Default: (a).
5. Negative hex/bin/oct output (`0x-ff`): (a) copy it, (b) print `-0xff`. Default: (b).
6. Global hotkey: Numi 3.35 ships none (§12.4). (a) none, as in Numi, (b) a default such as Ctrl+Opt+Space. Default: (a).
7. Document storage: Numi keeps one `.txt` per calculation in iCloud Drive or Application Support (§12.9). (a) local folder of UTF-8 `.txt` files, (b) the same plus a user-chosen synced folder later. Default: (a).
8. First-launch theme: Numi defaults to night mode (`nightTheme = true`). (a) dark by default, (b) follow the system. Default: (a).
9. Peripheral features: the Today widget (`NumiExtension.appex`), the `numi://` scheme (registered, no handler found), QuickType hints (no UI in 3.35), Sparkle updates, licensing and telemetry. (a) skip all, (b) add update checks only. Default: (a).
10. Worth one short session on a real Numi 3.35: `20%` vs `20 %`; what `today` prints; one-argument `log 100`; the unit of `sum` over mixed units; `x / 20%`; `2:30 pm` against the label regex; the result unit of `$5 * 2 EUR`.
