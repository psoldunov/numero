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

- Numbers are decimal, not binary floating point [obs]. All arithmetic uses `NSDecimalNumber` (128-bit mantissa, base-10 exponent -128..127). Every arithmetic call passes an `NSDecimalNumberHandler(roundingMode: .up (2), scale: 38, raises nothing)` (e.g. `FUN_00062670`, `FUN_000ce960`). In practice that means 38 significant digits, rounding up at the 38th decimal place, and no exceptions: overflow, underflow and divide-by-zero give NaN.
  - [oracle] The mantissa is the full 128 bits, so a 39-digit value whose mantissa fits stays exact: `10^38 + 1` → `100,000,000,000,000,000,000,000,000,000,000,000,001`, and a 40-digit literal ending in 0 prints every digit. Integer powers and long literals are exact (`10^39`, `2^100`, a 30-digit literal); `10^30 / 3` keeps 38 digits.
  - Rust: Numero computes with `rust_decimal` (28 digits) and moves to its own 128-bit-mantissa decimal (`src/big.rs`) when a result overflows, keeping a NaN / "no result" state.
- Transcendental functions and a few operators go through `f64` [obs]: trig, hyperbolic, `ln`, `log`, `root`, `cbrt`, `sqrt`, `fact` (`tgamma`), `mod`, non-integer `^` (`pow`), the degree ratio (π/180 as a double), and `pi` itself (`NSDecimalNumber(double: M_PI)`). Their results are converted back to decimal from the double, so they carry about 15-17 significant digits.
  - [oracle] The conversion is Foundation's `Decimal(Double)`: multiply the absolute value by 10 until it reaches 2⁶⁴, divide by 10 while it is at least 2⁶⁴, truncate to a `UInt64` mantissa, keep the exponent. Every step rounds in binary, so the low digits are invented: `fact(25)` → `15,511,210,043,330,981,888,000,000`, `fact(30)` → `265,252,859,812,191,078,400,000,000,000,000`, `tan(90)` → `16,331,239,353,195,368.45`. Numero emulates it (`Num::from_double`).
- A Value is a number plus a string-keyed attribute bag (`Value.attributes`) [obs]. Keys seen: `decimal` (the number), `unit` (unit id), `nsystem` (`hex`/`oct`/`bin`/`sci`, output radix), `date`, `time_zone`, `operation`, `opPriority`, `scale`, `percent`, `geo_name`. A value with no attributes is "empty" (the line has no result). `sum`/`avg` depend on this, see §5.
- Per-line pipeline [obs, names from symbols]: extractors tokenise the line into typed elements; matchers group tokens into values and expressions (`DecimalFollowedByUnitMatcher`, `UnitFollowedByDecimalMatcher`, `UnitAroundDecimalMatcher`, `ScaledExpressionMatcher`, `FunctionExpressionMatcher`, `ExplicitUnitConversionMatcher`, `TimePrefixedByTimeZoneMatcher`, `GeoNameSequenceMatcher`); evaluators fold operators by priority (§2). The document is evaluated top to bottom with one shared `Context` (variables, `prev_result`, `prev_results`, `grand_total`, `current_time`, `ppi`, `em`, constants).
- Initialisation order (`FUN_00063ce0`) [obs]: operators (`FUN_0004ea10`), operations (`FUN_0004ff50`), extractors (`FUN_0004c1f0`), matchers/evaluators (`FUN_0004b750`), formatters, units (`FUN_00051d60`), functions (`FUN_0004db90`), currency (`FUN_000476b0`), crypto (`FUN_00048af0`), dates (`FUN_00049a10`), scales (`FUN_00051370`), constants (`FUN_00046370`), CSS (`FUN_00046930`).

---

## 1. Units

### 1.1 Unit model [obs: `Unit` ivars, e.g. `FUN_00052480`]

Each unit has `id`, `phrases` (input words), `baseUnitId`, `baseRatio` (decimal), `format` (output template), `isSiUnit` (gets SI-prefixed siblings), `internationalSymbol` (English `.symbol`), `localizedSymbol` (current-locale `.symbol`), and optional `toBaseValueConverter` / `fromBaseValueConverter` for non-linear or context-dependent units.

- Same-base conversion: `in_base = value * baseRatio`, `in_target = in_base / target.baseRatio`. The ratio is "base units per one of this unit" (inch = 0.0254, base `m`).
- Temperature, CSS and currency use converter functions instead of the ratio.
- Phrases come from `<id>.variants` in `L/<Category>.strings` (comma-separated). When the UI locale is not English, the English table is merged in as well [obs: `FUN_0007e710` param 5]. Units also get lower-cased copies of their phrases [inf: param 6 calls `lowercased()`]. Phrase collisions log `Duplicate unit for phrase '…': previously '…', adding as '…'` (`FUN_0009f630`) [obs]; the unit registered last wins [oracle], which is why crypto coins (`C`, `F`, `B`, `K`, `GMT`, `JST`…) take those phrases over once `coins2.json` loads (Numero does not copy that, see Deliberate deviations).
- Matching case [oracle]: a phrase is first matched with its own case; when nothing matches, the lower-cased text is tried, so `1 KB` → `1 kb` (kilobit), `1 Kb in b` → `1,000 b`, `5 KM` → `5 km`, `1 NM` → `1 nm`, `5 G` → `5 g`, `5 T` → `5 t` (offline). `5 mb`, `5 gb`, `5 kib` match nothing. Currency codes also match in lower case (`5 usd` → `$5`, `5 try` → `₺5`, `5 all` → `5 L`, the lek) unless the spelling already means a unit (`5 cup` → `5 cup`).
- Phrases in the tables that never match [oracle]: `°C` and `°F` (`25 °C` → `25 °`: the angle `°`, then a stray `C`), and `K` for kelvin (`K` is the thousand scale: `300 K in F` → `300,000 °F` offline). Only the words `celsius`/`fahrenheit`/`kelvin` and the letters `C`/`F` reach temperature. There is no `deg` phrase (`1 deg` → `1`).
- Unit placement [oracle]: a unit right after another unit, a conversion target or `%` is ignored (`5 m kg` → `5 m`, `37 degrees celsius` → `37 °`, `10 % m` → `10 %`). A unit before a number applies to it (`km 5` → `5 km`, `kg 5 + 1` → `6 kg`); `at` is the attotonne, so `5 at 3` and `Meeting at 5` → `5 at`, and `at 2:30 pm` has no result.
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
| `litre` | 0.001 | yes, negative ones included (ml, cl, dl, hl, kl…); `.symbol` `l`. Display [oracle]: the litre itself is `L`, prefixed ones use the symbol: `1 l in ml` → `1,000 ml`, `1 hl in l` → `100 L`, `1 mL` → `1 ml` |
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
- Phrases include bare `C`, `F`, `K` (`L/Temperature.strings`). [oracle] `K` never reaches kelvin (it is the thousand scale) and `°C`/`°F` never match (§1.1).

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
  - [oracle] A power written with a superscript (`km²`, `m³`) behaves differently under roots: `sqrt(16 km²)` → `4 km`, `cbrt(27 m³)` → `3 m`, while `sqrt(16 m2)`, `sqrt(16 square meter)` and `sqrt(16 sq m)` → `4 m²`. Everywhere else the two spellings are the same unit. `1 m 2 kg` → `1 m²` (the template `%@ 2`, then a stray `kg`).

### 1.4 SI and IEC prefixes [obs: tables `FUN_000d1720` (SI), `FUN_000d0580` (IEC); expansion `FUN_00055c10`, `FUN_0005b4e0`]

SI (name, symbol, power of 10): yocto y -24, zepto z -21, atto a -18, femto f -15, pico p -12, nano n -9, micro µ -6, milli m -3, centi c -2, deci d -1, deca da 1, hecto h 2, kilo k 3, mega M 6, giga G 9, tera T 12, peta P 15, exa E 18, zetta Z 21, yotta Y 24. Localized symbols come from `L/SIPrefixes.strings`. The code uses `μ` (U+03BC) for micro and the strings file uses `µ` (U+00B5); accept both.

IEC (name, symbol, power of 1024): kibi Ki 1, mebi Mi 2, gibi Gi 3, tebi Ti 4, pebi Pi 5, exbi Ei 6, zebi Zi 7, yobi Yi 8.

- Each prefix on an `isSiUnit` unit yields a unit with ratio `base.ratio * 10^p` (or `1024^p`), phrases `prefixName + phrase` (`kilometer`, `kilometre`) and `prefixSymbol + symbol` (`km`). The concatenation is [obs]; the exact id string is [inf].
- `FUN_00055c10(unit, allowNegative)`: with the flag off, only prefixes with p ≥ 0 are generated. Data units pass false, so there is no `mb` (millibit). All other SI units pass true.
- [oracle] Prefixed symbols that look like words still exist: `pm` is the picometre (`1 nm in pm` → `1,000 pm`; after a number from 1 to 12 it is still the meridiem), `nm` the nanometre (the nautical mile is `nmi`), `at` the attotonne.
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

Anything no extractor claims is free text and is ignored, so `$9 for coffee` evaluates `$9` [inf; matches documented Numi behaviour]. Unknown words between two values do not break the expression: the second value is dropped (`5 for 3` → `5`, §2.3) [oracle].

Token details the oracle settled [oracle]:

- A second decimal point ends a number: `1.5.5` → `1.5`, `3.14.15 * 2` → `6.28`. `1,5` is still `15`.
- The letter `x` multiplies after a number, a `)` or a unit (`3 x 4 m` → `12 m`, `2 m x 3` → `6 m`); glued to a following digit it is a multiplication with no left side (`x2` → `2`).
- `GMT±n` (`GMT+3`, `gmt -5`) is one token whose offset is lost: alone the line has no result, after a time it is ignored (`3pm GMT+3` → `3:00 PM`), as a conversion target it is plain GMT (`now in GMT-5` gives UTC time). `UTC+3` is the zone `UTC` followed by `+ 3`, which adds three seconds (§8.2).
- Variables match ignoring case and may contain spaces (§5). The longest assigned name wins. Operator words beat a variable of the same name (`in = 4` assigns, but `in * 2` → `2`); units and constants do not (`m = 5` then `m * 2` → `10`, `pi = 3` then `pi * 2` → `6`).

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
- Parentheses are grouped and evaluated as sub-expressions [inf: `ParenthesesExtractor` plus an `opPriority` attribute that overrides operator priority when present]. [oracle] A missing `)` leaves the line without a result (`(1 + 2`, `sqrt(16`), and so does `foo()`. A group with `;` keeps its first value (`(2; 3)` → `2`).
- [oracle] A run of minus signs straight before a number negates once: `--5`, `- -5` and `---5` → `-5`, while `-(-5)` → `5` and `3--2` → `5`.
- Juxtaposition [oracle]: a value followed directly by a parenthesised group multiplies, binding tighter than `^`: `2(3)` → `6`, `2 (3 + 1)` → `8`, `(2)(3)` → `6`, `x(2)` → `8`, `3(2+1)^2` → `81`, `5 m (3)` → `15 m`. A number with a unit right after a value that ends in a unit adds: `1 h 30 min` → `90 min`, `1 m 20 cm` → `120 cm`, `1 m 30 kg` → no result. Any other value after a value is dropped and the first one wins: `2 pi` → `2`, `pi 2` → `3.14`, `x x` → `4`, `(2)3` → `2`, `10 % 3` → `10 %`, `10 % 3 + 1` → `11 %`, `2 x + 1` → `3`.
- Dangling operators are dropped [oracle]: `* 2`, `/ 2`, `+ 2` → `2`; `5 +`, `5 *` → `5`.

### 2.4 Arithmetic with units (operation implementations, `FUN_0004ff50`) [obs]

Each operator id has several registered implementations, tried in order; the first that accepts the operand types runs [inf: the "priority 100" argument is a registration weight].

- `+`: DateAndDecimalAddition (`FUN_000c45e0`, date + duration; the duration is converted to seconds and rounded), PercentageAddition (`FUN_000ca200`), DecimalAddition (`FUN_000c5880`).
- `-`: DateSubtraction (`FUN_000c5180`, date - date = seconds as a double, unit `second`), DateAndDecimalSubtraction, PercentageSubtraction (`FUN_000ca830`), DecimalSubtraction (`FUN_000c5eb0`).
- `*`: PercentageMultiplication (`FUN_000ca590`), DecimalMultiplication (`FUN_000c5d80`).
- `/`: DecimalDivision (`FUN_000c5ad0`).
- `^`: Exponent (`FUN_000c6240`). An integer exponent uses exact decimal `raisingToPower`; anything else uses `pow(f64, f64)`.
- `mod`: `FUN_000c6650`, computed in f64 [inf: C `fmod` semantics, so the result takes the dividend's sign].
- `&`, `|`, `xor`, `<<`, `>>`: on `integerValue`, truncated to i64 [obs: `FUN_000c64d0`].

Unit rules [oracle] (the earlier reading, "left unit wins", was wrong):

- `a + b`, `a - b`: `a` is converted to `b`'s unit and the result keeps `b`'s unit: `5 m + 3 ft` → `19.4 ft`, `3 ft + 5 m` → `5.91 m`, `1 m + 1 cm` → `101 cm`, `1 km - 1 m` → `999 m`, `0 celsius + 10 kelvin` → `283.15 K`, `$5 + 3 EUR` → euros. If only one side has a unit, the result takes it (`5 m + 3` → `8 m`, `3 + 5 m` → `8 m`, `$5 + 3` → `$8`). A percentage on the right is relative (§3).
- `a * b`: plain product in `b`'s unit when `b` has one, otherwise in `a`'s; nothing is converted or compounded: `2 m * 3 m` → `6 m`, `5 m * 2 cm` → `10 cm`, `2 m * 3 kg` → `6 kg`, `5 kg * 2 m2` → `10 m²`, `$5 * 2 EUR` → `€ 10`, `20% * 10 m` → `200 m`, `20% * 50` → `1,000 %`.
- `a / b`: by a plain number or a percentage the left unit stays (`10 m / 2` → `5 m`, `20% / 5` → `4 %`, `50 / 20%` → `2.5`); by a value with a unit both units drop and nothing is converted (`5 m / 2 cm` → `2.5`, `1 km / 1 m` → `1`, `6 m / 2 s` → `3`, `10 / 2 m` → `5`, `20% / 5%` → `4`).
- `mod`: remainder of the bare numbers in the left unit (`10 m mod 3 cm` → `1 m`, `7 mod 2 m` → `1`).
- `^`: the unit is kept, not raised (`(2 m)^2` → `4 m`).
- The numeral system follows the same rule: the right operand's wins when it has one (`1 + 0xff` and `0xff + 1` → `0x100`, `0xff + 1e3` → `1.26E3`, `1e3 + 0xff` → `0x4e7`).
- Incompatible units (`5 m + 3 kg`, `1 m + 1 m2`): the conversion fails and the line has no result.

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
| `x * p%` | PercentageMultiplication `FUN_000ca590` | `x * p/100`, in x's unit (also for `p% * q%` → percent) | `50 * 20%` = 10, `10 m * 20%` = `2 m`, `20% * 50%` = `10 %` [oracle] |
| `p% * x` | DecimalMultiplication | plain product in x's unit, or percent when x has none | `20% * 50` = `1,000 %`, `20% * 10 m` = `200 m` [oracle] |
| `x / p%` | none registered | DecimalDivision: `x / p` with the percent as a plain number | `50 / 20%` = `2.5` [oracle] |
| `p%` alone | value with unit `percent` | – | `20%` |

- The sample document (`L/Samples.strings`) has `8 / (45 - 20%)` = 8 / 36 ≈ 0.22.
- Percent is a unit with ratio 1, so conversions only happen inside the `percent` base: `0.2 in %` → `0.2 %` [oracle].
- `rate = 5%` then `rate of 200` → `10`: a variable holding a percentage works on the left of `of`/`on`/`off` [oracle].

---

## 4. Numbers

### 4.1 Literals [obs: regexes in §2.1]

- Decimal: `1234`, `1 234`, `1_234`, `1,234.5`, `.5`. The regex accepts `,` `.` `_` and spaces inside a number; the user's locale decides which is the decimal separator (en_US: `,` groups, `.` is decimal) [inf]. Underscores and spaces always group [inf].
- Hex `0xFF` (either case), binary `0b1010`, octal `0o17`; the value is the integer. The result keeps its numeral system (`nsystem`), so `0xFF + 1` prints in hex [inf from how attributes propagate].
- Scientific: `1.5e3`, `2E-4`, `1 000e3`.

### 4.2 Output representation (`in hex` etc.)

- `<expr> in hex | binary | bin | octal | decimal | dec | sci | scientific | exp | exponent | exponential` sets `nsystem` to `hex`, `bin`, `oct`, nothing (decimal), or `sci` [obs: `FUN_000a1680`].
- Rendering [obs: `FUN_000ac360`, `FUN_000ae770`]:
  - hex is `"0x"` followed by the lowercase base-16 digits of `integerValue` (the fraction is truncated). Binary uses `0b`, octal `0o`. For negative values the prefix is glued in front of Swift's signed `String(radix:)` output, giving `0x-ff` [obs]. [oracle] The display is `-0xff` after all: the minus goes in front of the whole value, as for any negative number (§7); `-0.001 in hex` → `-0x0`.
  - [oracle] A numeral-system word anywhere but after a conversion word: after a value it is ignored (`5 hex` → `5`), before one the line has no result (`hex 5`, `exp(2)`; `exp` is a scientific-notation word, there is no `exp()` function). `oct` is October, so `255 in oct` has no result (§8.1).
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

- Variables [obs: `AssignmentOperationMatcher`, context keys `var_<name>`]: `name = expr`, `name equal expr`, `name is expr`. Values keep their units, and the assignment line shows the assigned value [inf]. [oracle] Names ignore case (`Price = 10` then `price` → `10`) and may contain spaces (`total cost = 5`, `a b c = 3`, `The total is 5` assigns `the total`); the longest assigned name wins (`x y = 2`, `x y z = 3`, then `x y z` → `3`). A name needs a letter or `_` to be stored, but the line shows the right-hand side regardless (`5 = 3` → `3`, `a + b = 5` → `5`). `1x = 2` shows `2`, yet `1x` still reads as `1` followed by `x`. `x := 6` shows `6` and assigns nothing (`x :` is a label).
- `prev` [obs: context `prev_result`, set after every line in `FUN_000650f0`] is the result of the line directly above. [oracle] If that line had no result, `prev` alone has no result and leaves the grand total alone, and `prev + 1` → `1`.
- `sum`/`total` and `avg`/`average` [obs: `FUN_000b09c0`, `FUN_000b40b0`]: every line appends its result, empty or not, to `prev_results`. `sum` walks backwards from the previous line and collects values until it meets an empty one (a blank line, header, comment-only line, or a line that failed). It then folds the collected values with the normal `+` operation, so unit conversion applies and incompatible units make the sum fail. `avg` is that sum divided by the count.
  - [oracle] Because `+` keeps the right operand's unit, the bottom line's unit wins: `1 m`, `50 cm`, `sum` → `150 cm`; `50 cm`, `1 m`, `sum` → `1.5 m`; `1 h`, `30 min`, `1 day`, `sum` → `1 day 1 h 30 min`. `sum` or `avg` with nothing above has no result. A `sum` line is part of the block below it: `1 m`, `2 m`, `sum`, `avg` → `2 m`.
- Grand total [obs: context `grand_total`, `grand_total_formatted`]: a running `+` over every line result in the document, shown in the bottom bar. [oracle] Lines are not skipped when the addition fails. Each line does one of four things:
  - a result is added (so the bottom line's unit wins, and `today` plus `5` is `today` plus 5 seconds);
  - a line with nothing to calculate (blank, comment, header, unparsable text such as `sqrt of 16`) is skipped;
  - a failed calculation (`5 m in kg`, `1 / 0`, an empty `sum`) or a result the total cannot add (`1 m` then `1 kg`) empties the total for the rest of the document;
  - a line Numi gives an empty value (a function called with the wrong arguments such as `log 100`, or `5 m in foo`) starts the total over: `5`, `log 100`, `3` → `3`.
- Headers: a line matching `\s*#.*` is a header, styled, with no result [obs]. It ends a `sum` block because it produces an empty value [inf].
- Labels: in `Label: 5` the `Label:` part is ignored and the result is 5 [obs regex].
- Comments: `//` to end of line, and `"quoted text"` anywhere in a line [obs]. A comment-only line has no result.

---

## 6. Conversion rules

- Conversion words: `in`, `into`, `as`, `to` [obs]. The target can be a unit, a currency, `percent` [inf], a numeral system (§4.2), a time zone (§8.3) or a place name (GeoNames).
- [oracle] A conversion word followed by an unknown word fails the line (`5 m in foo`, `pi in deg`, `1 sq ft in sq in`), and so does a month (`255 in oct`). Followed by a known phrase that is not a target, or by nothing, it is ignored (`5 m in K` → `5 m`, `5 m in` → `5 m`, `1 sq in in cm2` → `1 cm²`). A bare number takes the target unit (`5 in km` → `5 km`, `pi in rad` → `3.14 rad`).
- `ConversionOperation` only converts within one base unit. Currency goes through USD (§1.2).
- Mixed-unit arithmetic takes the right operand's unit (§2.4) [oracle].
- Mixed currencies: `$5 + 3 EUR` gives a USD result at the latest rates. With no rates (offline before the first fetch) the converters return NaN and the line has no result [obs: converters return `notANumber`].
- A failed conversion between incompatible categories leaves the line without a result; Numi shows nothing rather than an error message [inf].

---

## 7. Formatting [obs: `FUN_000ae5e0` formatter init, `FUN_000ac360` value formatter, `FUN_000ae770`]

- Two `NSNumberFormatter`s on the user's locale: decimal style (`numberStyle = 1`) and scientific style (`4`), both with `roundingMode = 6`, which is half-up.
- Max fraction digits = context `precision`, 2 when unset [obs: fallback `lVar14 = 2`]. The app's precision preference sets it (see `spec-ui.md`).
- Min fraction digits is 0, except for currencies (units whose base is `USD`): if the number has exactly one fractional digit, the minimum becomes 2. So `$1.5` shows `$1.50`, while `$1.25` and `$1` stay as they are [obs: `(fractionDigitsCount == 1) * 2`]. [oracle] The count is taken on the unrounded value, so an amount that only rounds to one digit keeps one: `1 ETH in USD` → `$2,430.4`.
- Grouping follows the locale's decimal style (en_US `1,234,567.89`) [obs: decimal style groups by default].
- There is no automatic switch to scientific notation [inf]. The decimal formatter is used unless `nsystem = sci`, so tiny numbers show as `0` at 2 decimals and huge numbers print every digit. [oracle] Confirmed for the regular display; the `short` variant does switch (`1000000.5` → `1E6`).
- Negative numbers are formatted as their absolute value, then `-` goes in front of the whole unit template: `-$5`, `-5 m` [obs: subtract from zero, then prepend `-`]. [oracle] The sign stays on a value that rounds to zero: `-0.001` → `-0`, `-0.001 m` → `-0 m`; the literal `-0` is `0`. Rounding is half-up in decimal: `0.125` → `0.13`, `-0.005` → `-0.01`.
- Time values with a fractional amount [oracle] are spelled out from their own unit down to whole seconds along `yr` (365 d), `mon.` (30 d), `w`, `day`, `h`, `min`, `s`: `90 min in hours` → `1 h 30 min`, `1 year in months` → `12 mon. 5 day`, `400 days in years` → `1 yr 1 mon. 5 day`, `1 h in days` → `0 day 1 h`. Zero parts are skipped except the first; sub-second remainders are dropped (`1.999 h in h` → `1 h 59 min 56 s`). The sign goes on every non-zero part: `-90 min in h` → `-1 h -30 min`, `-30 min in h` → `0 h -30 min`. Whole amounts print plainly (`2.5 days in hours` → `60 h`), and so do seconds and prefixed units (`1.5 s`, `1.5 ms`).
- Unit rendering follows §1.1. Percent's format is `%%`, and the rule turns it into `"<n> %%"`, which `String(format:)` collapses to `"20 %"`. Numi shows `20 %`, with an ASCII space [oracle].
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

[oracle] Corrections and additions to the table:

- `today` is today at 12:00 local time, not the current instant; `tomorrow`/`yesterday` are ±86,400 s from it. `today + 1 hour` → `1:00 PM`, `today + 30 days` → `11/13/26, 11:00 AM` (the 30 days cross the end of DST).
- A clock time written without am/pm and without a leading zero (`2:30`, `9:00`, `11:59`) reads as PM when its morning time has already passed and its afternoon time falls in the current hour or later; otherwise AM. At 9:41 PM: `9:00`, `9:59`, `10:05`, `11:59` → PM; `1:00`, `2:30`, `8:00` → AM. `09:30` and `00:30` are AM, `12:30` is 12:30 PM, `13:00` is 1 PM. The test is made on local time even for a zoned time (`8:00 UTC` → 10:00 AM Berlin, `10:00 UTC` → midnight).
- `noon` and `midnight` are not phrases.
- Month names (the en_US calendar's long and short forms, plus `sept`) with a day make a date at 12:00 local: `5 oct`, `oct 5` → `10/5/26`, `dec 5` → `12/5/26`, `oct 5 + 1 day` → `10/6/26`. `may` only works first (`may 5` → a date, `5 may` → `5`). Days past the month's end roll over (`31 feb` → `3/3/27`). The year is the current one unless that date lies far in the past: 39 days back stays in this year, 162 days back moves to next year (Numero uses 90 days as the cut-off). A month alone has no result.

### 8.2 Arithmetic [obs]

- `date ± duration`: the duration is converted to seconds with the fixed ratios from §1.2, rounded, and added (`FUN_000c45e0`). `today + 1 month` adds 30 days and `+ 1 year` adds 365 days; there is no calendar arithmetic. The rounding mode and scale used here are [unk]; default to whole seconds, half-up.
- [oracle] A plain number added to or subtracted from a date counts as seconds: `today + 60` → `12:01 PM`, `3 + today` → `10/14/26`.
- `date - date` gives seconds as a `second` value, computed in f64 (`FUN_000c5180`). Convert with `in days` etc. [oracle] The result is shown in the largest unit of `yr`, `mon.`, `w`, `day`, `h`, `min`, `s` that it fills at least once, then spelled out as in §7: `tomorrow - today` → `1 day`, `5pm - 9am` → `8 h`, `9am - 5pm` → `-8 h`, `(today + 10 days) - today` → `1 w 3 day`, `(today + 45 days) - today` → `1 mon. 2 w 1 day`, `(now + 90 min) - now` → `1 h 30 min`. A zero difference is `0 s`.
- `fromunix(x)` / `unixtime(x)` [obs: phrases, `FUN_000b1fd0`]: date from Unix seconds, read as UTC and shown in local time. [oracle] Numi shifts by today's UTC offset rather than the offset at `x` (`fromunix(0)` in Berlin → `12/31/69, 11:00 PM`); Numero uses the offset at `x` (Deliberate deviations).
- `gmt` [obs: `FUN_000b24b0`]: `GMT+3` or `gmt -5` gives a fixed-offset zone of `n * 3600` seconds. Fractional hours are truncated to whole seconds [inf]. [oracle] In practice the offset never shows: see §2.1. `now in GMT+3` gives UTC time, `3pm GMT+3` is 3 PM local, and `GMT+3` alone has no result.

### 8.3 Time zones

- Abbreviations [obs: `FUN_000a9c20` reads Foundation's `TimeZone.abbreviationDictionary`, skips `GMT`, adds lower-cased phrases]. The table belongs to the OS, not to Numi. Embed a copy of the macOS dictionary [inf: from Foundation, verify on macOS]: ADT, AKDT, AKST, ART, AST, BDT, BRST, BRT, BST, CAT, CDT, CEST, CET, CLST, CLT, COT, CST, EAT, EDT, EEST, EET, EST, GMT, GST, HKT, HST, ICT, IRST, IST, JST, KST, MDT, MSD, MSK, MST, NZDT, NZST, PDT, PET, PHT, PKT, PST, SGT, TRT, UTC, WAT, WEST, WET, WIT, each mapped to an IANA id (`CET→Europe/Paris`, `EST→America/New_York`, `PST→America/Los_Angeles`, `HKT→Asia/Hong_Kong`, `IST→Asia/Kolkata`, `JST→Asia/Tokyo`, `BST→Europe/London`, `MSK→Europe/Moscow`, …). IST, CST and BST are ambiguous; follow the macOS mapping.
- A time with a zone (`2:30 pm HKT`) goes through `TimePrefixedByTimeZoneMatcher`; `… in Berlin` or `in EST` converts through `TimeZoneConversionOperation`. [oracle] The zoned time is read on today's local date and shown in the local zone: `3pm EST` → `9:00 PM`, `2:30 pm HKT` → `8:30 AM`. `in <zone>` converts only `now`/`time` (`now in PST` → `12:22 PM`, `now in local`); after any other time, a date or a number it is ignored (`3pm EST in PST` → `9:00 PM`, `2:30 pm in HKT` → `2:30 PM`, `today in UTC` → `10/14/26`, `5 in PST` → `5`). A zone after a number is ignored too (`5 PST` → `5`).
- City and place names [obs: `GeoNamesService.timeZoneIDForPlace`, `GeoNameSequenceMatcher`, `postCalculationHandleGeoNames`]: unknown words in a time context are sent to `https://s1.numi.app/timezone?q=<place>` (query parameter `q`), and the response yields an IANA zone id [inf: plain id or JSON, not seen]. Results are cached and the line is recalculated when the async reply arrives [inf]. HTTP 503 is retried up to `maxRetryCount = 4` with a "retry in …" delay [obs]. Offline, only abbreviations, `GMT±n` and `local` work. For Numero, ship an offline city→IANA table (e.g. GeoNames cities15000) instead of calling a server.

### 8.4 Output format [obs: `FUN_000ab780` creates three `DateFormatter`s on the user's locale; selection in `FUN_000aba90`]

| formatter | dateStyle | timeStyle | en_US example |
|---|---|---|---|
| A | short | short | `10/14/26, 3:45 PM` |
| B | none | short | `3:45 PM` |
| C | short | none | `10/14/26` |

Selection [inf from the branch structure, which calls two calendar predicates `FUN_00095c10`/`FUN_00095d40`]: a value whose time falls on the current day uses B (`now`, `time`, `2:30 pm HKT in Berlin`); any other day uses A for `regular` and C for `short`. [oracle] The two predicates are "time is 12:00" and "date is today": a value at 12:00 (seconds ignored) shows only its date, which is how `today`, `tomorrow`, `12:00` and `today + 1 week` read; otherwise a value whose date, in the zone it is shown in, equals today's local date shows only the time; anything else shows both (`now in JST` → `10/15/26, 4:22 AM` in the evening in Berlin). macOS 27 puts U+202F before AM/PM; Numero uses a plain space (Deliberate deviations).

---

## 9. Functions [obs: registration `FUN_0004db90`, phrase arrays decoded; implementations `FUN_000af…`–`FUN_000b5…`]

Call syntax is `name(x)` or `name x`. Multi-argument functions take a sequence of values; `Contents/Resources/Sample.js` separates them with `;` (`zum(2;3)`). The engine has no separator literal [obs], so `;` is free text that happens to split the numbers [inf]. `log 2 (8)` reads the value before the parenthesis as the first argument [inf].

Call rules [oracle]:

- A function name followed by a minus sign without parentheses fails (`abs -5`, `abs - 5`, `cbrt -8`); `abs(-5)` works. A function name with no value after it fails the line (`sqrt of 16`, `abs of -5`).
- `log` needs its base: `log 100`, `log(100)`, `log 2 8` have no result; `log(8; 2)` → `0.33` (log₈ 2), `log(2)(8)`, `log(2) (8)` and `log2(8)` → `3`, `log 0.5 (4)` → `-2`. `root 3 27` has no result; `root(3; 27)` → `3`. A wrong argument count gives the line Numi's empty value (§5).
- An unknown name before `(` is free text: `foo(1)` → `1`, `pow(2; 3)` → `2` (the first value wins), `foo()` → no result.

| function | phrases | args | behaviour |
|---|---|---|---|
| sqrt | `sqrt, square root` | 1 | √x (f64), keeping the unit: `sqrt(16 m)` → `4 m`, `sqrt 2 m` → `1.41 m`, `sqrt(16 m2)` → `4 m²`, but `sqrt(16 km²)` → `4 km` (§1.3) [oracle] |
| cbrt | `cbrt, cubic root, cube root, cubed root` | 1 | `cbrt` (f64), unit as for sqrt (`cbrt(27 m³)` → `3 m`) [oracle] |
| root | `root` | 2 | `root n (x)` = x^(1/n); for x < 0 and odd n it returns `-((-x)^(1/n))` [obs `FUN_000b3430`] |
| abs | `abs` | 1 | `|x|` in decimal (`0 - x` when negative) |
| log | `log` | 2 | `log b (x)` = log2(x) / log2(b); base 1 gives NaN [obs `FUN_000b3140`]. One-argument `log x` has no result [oracle] |
| ln | `ln` | 1 | natural log; `ln 0` gives NaN [obs `FUN_000b2a40`] |
| fact | `fact` | 1 | `tgamma(x + 1)`, so non-integers work (`fact 0.5` ≈ 0.886) [obs `FUN_000b1c80`] |
| sin, cos, tan | `sin`, `cos`, `tan` | 1 | a unitless argument is in degrees; `rad`/`°` units are honoured; the result is unitless [obs `FUN_000b16c0` etc.: no unit → `degree` → converted to radian] |
| arcsin, arccos, arctan | `arcsin`, `arccos`, `arctan` | 1 | result in degrees (computed in radians, then converted) [obs `FUN_000afda0`], carrying the `°` unit: `arcsin(0.5)` → `30 °`, `arctan 1 in rad` → `0.79 rad` [oracle] |
| sinh, cosh, tanh | `sinh`, `cosh`, `tanh` | 1 | plain f64, no unit handling |
| round | `round` | 1 | to an integer with NSRoundPlain, i.e. half away from zero [obs `FUN_000b37b0`: scale 0, mode 0] |
| ceil, floor | `ceil`, `floor` | 1 | f64 |
| sum, avg, prev | §5 | 0 | |
| today, tomorrow, yesterday, time/now, local | §8 | 0 | |
| fromunix | `fromunix, unixtime` | 1 | §8.2 |
| gmt | `gmt` | 1 | §8.2 |

- There is no `pow()` or `exp()` function (use `^` and `e^x`) and `%` is never modulo [obs: not registered].
- Constants: §1.5.
- User extensions (JavaScriptCore, `JSNumi`): `numi.setVariable(name, {double, unitId} | number)`, `numi.addUnit({id, phrases, baseUnitId, format, ratio, internationalSymbol})`, `numi.addFunction({id, phrases}, fn(values) → {double, unitId?})` (`Contents/Resources/Sample.js`). Values cross that boundary as `{double, unitId}` (f64). Numero's `UnitSpec::international_symbol` takes `internationalSymbol`: like a built-in unit's `.symbol` (§1.1) it is one more input phrase and the symbol the unit is spelled with in ids; the output still follows `format`.

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
| Decimal arithmetic | 128-bit mantissa (38-39 digits), NaN instead of errors; f64 results through Foundation's `Decimal(Double)` | §0 | obs, oracle |
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
| Mixed-unit arithmetic (right unit wins for `+ − ×`, `/` by a unit drops units) | | §2.4 | oracle |
| 12 percent operations | of/on/off, …what is, as a % of/on/off, x ± p%, x × p% | §3 | obs |
| Variables with `=`, `equal`, `is` | | §5 | obs |
| `prev`, `sum`/`total`, `avg`/`average` | block ends at an empty line | §5 | obs |
| Grand total bar | | §5 | obs |
| Headers `#`, labels `…:`, comments `//` and `"…"` | | §2.1, §5 | obs |
| Free text ignored | | §2.1 | inf |
| Functions sqrt, cbrt, root, abs, log, ln, fact, trig (degrees), arc-trig, hyperbolic, round, ceil, floor | | §9 | obs |
| Dates today/tomorrow/yesterday/now/time, date arithmetic | fixed 30-day month, 365-day year | §8.1-8.2 | obs |
| `fromunix`/`unixtime`, `GMT±n`, `local` | `GMT±n` loses its offset | §8.2 | obs, oracle |
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
| `numi.addUnit(dict)` | Dict keys: `id`, `phrases` (comma list), `baseUnitId`, `format`, `ratio`, `internationalSymbol` (Numero: an extra phrase and the unit's symbol, §9) | `UI@0xb7cc0`, `Sample.js` | obs |
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
3. Associativity of `^`: the binary evaluates leftmost first (`2^3^2 = 64`). Answered [oracle]: Numi gives `64`, and `-2^2` → `4`. Numero copies it.
4. Implicit multiplication (`2(3)`, `2 pi`): answered [oracle]. Only a value followed by a parenthesised group multiplies; any other second value is dropped (§2.3). Numero copies it.
5. Negative hex/bin/oct output (`0x-ff`): answered [oracle]. Numi already prints `-0xff` (§4.2).
6. Global hotkey: Numi 3.35 ships none (§12.4). (a) none, as in Numi, (b) a default such as Ctrl+Opt+Space. Default: (a).
7. Document storage: Numi keeps one `.txt` per calculation in iCloud Drive or Application Support (§12.9). (a) local folder of UTF-8 `.txt` files, (b) the same plus a user-chosen synced folder later. Default: (a).
8. First-launch theme: Numi defaults to night mode (`nightTheme = true`). (a) dark by default, (b) follow the system. Default: (a).
9. Peripheral features: the Today widget (`NumiExtension.appex`), the `numi://` scheme (registered, no handler found), QuickType hints (no UI in 3.35), Sparkle updates, licensing and telemetry. (a) skip all, (b) add update checks only. Default: (a).
10. Answered by the oracle (`~/re/numi/oracle`, which runs Numi's own engine): `20 %` with a space; `today` prints the date (§8.1, §8.4); `log 100` has no result; `sum` takes the bottom line's unit; `50 / 20%` → `2.5`; `2:30 pm` is a time, also after `Meeting:`; `$5 * 2 EUR` → `€ 10`.

---

## Deliberate deviations from Numi

Numero matches Numi 3.35 as the oracle shows it, except here. `crates/numero-engine/tests/oracle.rs` lists every row these touch.

1. Crypto coin codes do not shadow other phrases. Once Numi loads `coins2.json`, coins named `C`, `F`, `K`, `B`, `T`, `G`, `S`, `GMT`, `JST` and others take those phrases over, so `0 C in F` converts between two coins, `1 kB in B` and `now in JST` have no result, and `5 G` is a coin. Numero keeps temperature, data units, scales and time zones; its results equal Numi's before the first rate fetch (the oracle's offline run).
2. Times use a plain space before AM/PM (`2:30 PM`). macOS 27's date formatter puts U+202F there.
3. `fromunix(x)` and `unixtime(x)` use the UTC offset in effect at `x`. Numi applies today's offset (`fromunix(0)` in Berlin in October → `12/31/69, 11:00 PM`), which is a bug: the instant shown is not the instant meant.
4. City names (`now in Berlin`, `time in Tokyo`) come from an offline table (§8.3). Numi asks `s1.numi.app` and had no answer during the oracle runs, so it showed nothing.
