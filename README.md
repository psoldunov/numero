# Numero

Numero is a text calculator for Linux and macOS. You write one calculation per line and the result shows up beside it. It is a clean-room reimplementation of [Numi](https://numi.app) 3.35 in Rust, with a GPUI interface. The engine was rebuilt from the behaviour of the shipped Mac app and then checked line by line against Numi's own engine. Numero is not affiliated with Numi or its author.

```
# Trip
Flight: $420
Hotel = 3 * €89
Hotel in USD
Tip: 15% of $86
sum

5 inches in cm
2:30 pm PST in Berlin
today + 3 weeks
```

## What it does

The engine understands what Numi does:

- arithmetic with words or symbols (`plus`, `times`, `mod`, `^`, bitwise `&` `|` `xor` `<<` `>>`), parentheses and Numi's operator priorities
- units of length, area, volume, weight, time, angle, temperature, data (SI and IEC prefixes) and CSS, converted with `in`, `into`, `as` or `to`
- about 160 currencies and the common crypto coins, with rates fetched from open.er-api.com and CoinGecko and cached for six hours
- the twelve percentage forms (`20% of 10`, `5% on 30`, `50 as a % of 100`, `5% of what is 6`, ...)
- variables, `prev`, `sum`/`total` and `avg`/`average`, headers (`#`), labels (`Rent:`), comments (`//` and `"quoted"`), and free text mixed into a line
- hex, binary, octal and scientific input and output, scales like `k`, `M` and `billion`
- functions (`sqrt`, `root`, `log`, `ln`, trig in degrees, `round`, `fact`, `fromunix`, ...), dates, and time zones by abbreviation or city name, offline
- unit vocabulary in the 27 languages Numi ships

The app has Numi's window: a custom title bar over a translucent, blurred background, the result column with click to copy, and the four corner buttons for a new calculation, the calculation list with search, preferences and export. It also has the preferences window, the menu bar mode with a tray icon, a global shortcut, launch at login, always on top, Numi's menus and shortcuts (Copy Result, Copy All, Surround with Parentheses, Delete All, Import, Export), the grand total bar and a resizable result column.

Calculations are plain `.txt` files, one per calculation, in a folder you can move into iCloud Drive, Syncthing or Dropbox. Numero watches the folder and writes a conflict copy instead of overwriting a file that changed on another machine.

Extensions written for Numi work too. Every `.js` file in the extensions folder runs at startup with the same `numi.setVariable`, `numi.addUnit` and `numi.addFunction` calls. Numero runs them in QuickJS on a separate thread, so a slow function can't freeze the editor.

`numero://new?text=...`, `numero://open?name=...` and `numero://show` open the app from a link.

## Where Numero differs from Numi

A few of Numi's behaviours are bugs, and Numero doesn't copy them. Crypto tickers like `C`, `F` and `GMT` don't take over temperature units or time zones. `fromunix` uses the time zone offset of the date it converts. Times use a plain space before AM/PM. The full list is at the end of [docs/numi-spec.md](docs/numi-spec.md).

Numero can't print yet, since GPUI has no printing API. There is no Today widget, no automatic update check, and the interface is English only (the engine still understands all 27 languages). These are in the backlog.

## Building

You need Nix. The dev shell brings the Rust toolchain and the libraries GPUI loads on Linux. On macOS it uses Xcode's SDK.

```sh
nix develop
just app          # run the app
just cli file.txt # print a document with its results in the terminal
just check        # formatting, clippy and every test
just mac-check    # from Linux: run `just check` on a Mac over SSH
```

Installing is covered in [docs/install.md](docs/install.md).

## Layout

| Crate | What it is |
|---|---|
| `crates/numero-engine` | the calculator: lexer, parser, units, currencies, dates, formatting |
| `crates/numero-rates` | fetches and caches exchange rates |
| `crates/numero-library` | the calculations folder: files, naming, watching, conflicts |
| `crates/numero-extensions` | Numi-compatible JavaScript extensions on QuickJS |
| `crates/numero-cli` | `numero`, the terminal front end |
| `crates/numero-app` | `numero-app`, the GPUI app |

The behaviour Numero copies is written down in [docs/numi-spec.md](docs/numi-spec.md) (engine) and [docs/numi-spec-ui.md](docs/numi-spec-ui.md) (app). `crates/numero-engine/tests/oracle.rs` checks the engine against results recorded from Numi's real engine.
