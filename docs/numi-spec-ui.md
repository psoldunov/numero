# Numi 3.35 (build 743): UI behaviour spec

Scope: preferences, hotkey, themes, result column, documents, menu-bar mode, window and menus.
Engine behaviour is covered by the other spec.

Tags: [obs] observed directly in artifact data or decompiled code, [inf] inferred from code shape
or names, [unk] not determined.

Evidence naming:
- `APP@0x1000xxxxx` is a function address in `thin/Numi.x86_64` (main app). The Ghidra dump is at
  `/home/psoldunov/re/numi/dump-app/decomp.c`; I produced it for this task with the existing
  `DumpAll.java`.
- `UI@0x0000xxxx` is a function address in `thin/NumiUI.x86_64` (`dump/decomp.c`).
- Nibs were decoded with my own NIBArchive parser, `/home/psoldunov/re/numi/scripts2/nib.py`
  (menus: `scripts2/menu.py`).

Bundle facts [obs] (`Contents/Info.plist`):
- Bundle ID `com.dmitrynikolaev.numi`, `LSUIElement=true`, minimum macOS 10.13.
- Document type `.numi` ("Numi Calculations", role Editor, not a package).
- URL scheme `numi`.
- Login helper `com.dmitrynikolaev.numi3helper` (`LSBackgroundOnly`).

---------------------------------------------------------------------------------------------

## 1. Preferences

### 1.1 The preferences window
Source: `Base.lproj/GeneralPreferences.nib` (class `GeneralPreferencesViewController`), shown in a
toolbar-style preferences window titled "Numi Preferences" with tabs "General" and "Extensions"
(`Localizable.strings`) [obs].

General tab controls in nib order [obs]:

| Label | Control | Outlet / action |
|---|---|---|
| "Hotkey:" | `SRRecorderControl` (ShortcutRecorder 3) | `shortcut` / delegate `shortcutRecorderDidEndRecording:` |
| "Precision:" | `NSTextField` with integer formatter | `precisionTextField` / `controlTextDidChange:` |
| "Night Mode" | checkbox | `darkThemeCheckbox` / `darkThemeCheckboxClicked:` |
| (font popup) | `NSPopUpButton` | `fontSizePopUp` / `fontSizePopUpChanged:` |
| "Always on Top" | checkbox | `alwaysOnTopCheckbox` / `alwaysOnTopCheckboxChanged:` |
| "Launch Numi at Login" | checkbox | `autostartCheckbox` / `autostartCheckboxClicked:` |
| "Show in Menu Bar" | checkbox | `menubarModeCheckbox` / `menubarModeCheckboxClicked:` |
| "Enable Alfred integration" | checkbox | `alfredIntegrationCheckbox` / `alfredIntegrationChanged:` (out of scope) |
| "Send Feedback" | bold button | `actionButton` / `sendFeedback:` (opens `mailto:mail@numi.app?subject=Numi%20feedback%20(<build>)`) |

- Font popup items, in order: "Dynamic Font", "Small Font", "Medium Font", "Large Font" [obs].
  The item index is the stored `fontSize` integer [inf]; `APP@0x100008ad0` returns a byte that is
  passed to `selectItemAtIndex:`.
- The precision field has an `NSNumberFormatter` [obs]: format `#,##0.###`, integers only
  (`allowsFloats=false`), min 0, max 10, no grouping. Its initial value is the stored integer
  (`APP@0x100007f80` reads `precision`).
- Extensions tab: "You can use JavaScript extensions to add custom constants, units or
  functions. Check out docs…" and an "Open extensions folder" button [obs].
- A License section exists ("License", "Trial. %i days left", "Trial is over. Buy now!"). A clone
  does not need it.

### 1.2 `NSUserDefaults` keys and defaults
Keys come from a Swift string-raw enum at `APP@0x100007270` with 16 cases [obs]: `precision`,
`fontSize`, `windowFrame`, `resultPaneWidth`, `shortcutObject`, `menuBarMode`, `userWindowFrame`,
`nightTheme`, `alwaysOnTop`, `activationOnUpdate`, `whatsNewVersion`, `SUUpdaterIsInDevMode`,
`correctFinish`, `isEngineCacheDisabled`. `alfredIntegration` and `hasLaunchedBefore` are used
as strings elsewhere.

`registerDefaults:` runs in `APP@0x100007470` [obs]. A second, unrelated `registerDefaults:` sets
`NSApplicationCrashOnExceptions=YES` (`APP@0x1000130d0`).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `precision` | Int | 2 | Decimal places. Written by `APP@0x100030b90`, which also pushes it into the view controller and re-renders. Valid range 0 to 10. |
| `fontSize` | Int | 0 | 0 Dynamic, 1 Small, 2 Medium, 3 Large (see 1.3). Applied to the window immediately. |
| `windowFrame` | Data (`NSKeyedArchiver` of `NSValue(rect)`) | 370x360, centred on the main screen | Main window frame. Origin is `((screenW-370)*0.5, (screenH-360)*0.5)` (constants -370, -360, 0.5 at 0x1000a04a0). |
| `userWindowFrame` | Data (same) | same as `windowFrame` | [unk] exact role. Probably the frame the user last set, as opposed to the frame saved around menu-bar popup mode. |
| `resultPaneWidth` | Int | 120 (px) | Width of the result column. |
| `menuBarMode` | Bool | false | "Show in Menu Bar". |
| `nightTheme` | Bool | true | "Night Mode". The default look is dark. |
| `alwaysOnTop` | Bool | false | Floating window. |
| `hasLaunchedBefore` | Bool | false | Set to true at the end of first launch (`APP@0x10002c870`). On first launch with an empty document store, the welcome sample is loaded. |
| `activationOnUpdate` | Bool | false | Licence flow. |
| `alfredIntegration` | Bool | false | Ignore. |
| `correctFinish` | Bool | true | [inf] "terminated cleanly" flag. It sits in the function with the "Rough termination" string (`APP@0x10002a5e0`). |
| `isEngineCacheDisabled` | Bool | false | Engine debug flag. |
| `shortcutObject` | Dictionary | no default registered | See section 2. |
| `whatsNewVersion`, `SU*` | | | Release notes and Sparkle. Ignore. |

Not stored in `NSUserDefaults` [obs]:
- Launch at login. It is read live from the helper (`isStartedAtLogin`) and written with
  `SMLoginItemSetEnabled` for `com.dmitrynikolaev.numi3helper` (`AutostartHelper::setStartAtLogin:`,
  `APP@0x100004fbd`).
- Currency refresh interval (no preference exists; see section 7).

There are no keys for thousands separator, decimal separator, locale override or grouping [obs].
The only number-format preference is `precision`. Separators therefore come from the engine and
`Locale.current` [unk, out of scope].

### 1.3 Font sizes
Applied by `CalculatorView.autoAdjustMetricsBasedOnContainerWidth` (`UI@0x0002e940`). The
constants are doubles at `UI@0x107bd8`, `0x108e30` and nearby [obs].

| Mode | Body font size | Notes |
|---|---|---|
| Small (1) | 13 | |
| Medium (2) | 15 | |
| Large (3) | 17 | |
| Dynamic (0) | driven by window width: up to 400 gives 13, over 400 gives 14, over 600 gives 15, over 800 gives 17 | thresholds at `UI@0x108e30`, `0x108e38`, `0x108e48` |

- Each mode also sets four more metrics. I could not name them with confidence (they look like
  row spacing and padding). Values: Small (10, 4, 5, 15), Medium (20, 5, 10, 20), Large
  (20, 5, 15, 25), Dynamic over 400 (13, 5, 8, 18) [obs values, roles unk].
- The font is `NSFont(name: <current font name>, size:)`. The default family is Menlo
  (`APP@0x10000dcc0`; fallback `systemFont(15)`) [obs].
- A change re-lays out the text and re-runs `adjustResultPaneWidthWithinBoundary`.

### 1.4 Result pane width
- Default 120 px. When the adjust flag is on, it is clamped to `[bounds.width/3, bounds.width/2]`
  (`UI@0x0002e860`, constants 3.0 and 0.5) [obs].
- Persisted under `resultPaneWidth` (`APP@0x100008460` reads, `APP@0x10002eec0` writes) [obs].
- [unk] How a drag on the separator is wired.

---------------------------------------------------------------------------------------------

## 2. Global hotkey

- Recorder: ShortcutRecorder 3 (`SRRecorderControl`). Allowed modifiers `0x1e0000` (Ctrl, Opt,
  Shift, Cmd), none required (`APP@0x10000d030`, `setAllowedModifierFlags:required:`) [obs].
- Storage: `NSUserDefaults["shortcutObject"]`, a `[String: Any]` dictionary produced by
  `SRShortcut.dictionaryRepresentation` [obs]. `APP@0x100027970` writes it, `APP@0x100008580`
  reads it, and `APP@0x10002eb80` loads it with `shortcutWithDictionary:`.
  - The SR3 dictionary keys are `keyCode`, `modifierFlags`, `characters` and
    `charactersIgnoringModifiers`, per the ShortcutRecorder library [inf]. Only `keyCode` appears
    as a string in the binary (`0x100094ad0`).
- Registration uses `SRGlobalShortcutMonitor.sharedMonitor`, rebuilt on every change:
  `removeAllActions`, then `addAction:forKeyEvent:` with key-down, whose block calls
  `AppController.hotkey:` [obs].
- Default: none. No code path registers or writes a default shortcut: no `registerDefaults`
  entry and no first-launch assignment [obs, negative result across the dump]. [unk] Whether
  the shipped app behaves differently through something outside the bundle.
- Behaviour [inf]: `AppController.hotkey:` runs the same toggle as a status-item left click. It
  shows the window and activates the app, or hides it if it is already frontmost.

---------------------------------------------------------------------------------------------

## 3. Themes

The class is `CalculatorTheme` in `NumiUI`. Factories: `defaultTheme` (`UI@0x242a0`, which calls
`0x24da0`), `nightTheme` (`UI@0x242d0`), `printTheme` (`UI@0x247d0`), `extensionTheme`
(`UI@0x24570`) [obs].

The user chooses between light ("Night Mode" off) and night (on). Print is used for printing
[inf]. The extension theme is for the share extension (`NumiExtension.appex`) [inf].

The 18 fields and their values, in 6-digit hex [obs]:

| Token | Light | Night |
|---|---|---|
| text | `677C8D` | `E3E7E9` |
| header (`# Heading` lines) | `146788` | `FFC22D` |
| label (`Label:`) | `146788` | `FFC22D` |
| number | same as text | same as text |
| unit | `84A9C5` | `BDD4D2` |
| comment | `DBDBE2` | `62626B` |
| variable | `33A9E4` | `6CC4EE` |
| variableAssignment | same as variable | same as variable |
| operation (words like `in`, `of`, and `+`) | `33A9E4` | `6CC4EE` |
| operationSymbol | same as text | same as text |
| function | `33A9E4` | `6CC4EE` |
| separator (line between text and results) | `DDDDDD` | `3D3D3D` |
| selectionBackground | `B2D7FF` | `50525D` |
| insertionPoint (caret) | same as text | same as text |
| result | `5EC930` | `8DD32D` |
| resultOver (text colour on hover) | `FFFFFF` | `27282D` |
| resultOverBackground (hover pill) | `30CA41` | `8DD32D` |
| resultClickedBackground | `2AB439` | `75B220` |

- Print theme: every field is `NSColor.black` [obs].
- Extension theme: text tokens use `NSColor.labelColor`, the separator is `C5C5C5` at the alpha in
  `DAT_00108c68`, selection is `selectedTextBackgroundColor`, hover backgrounds are `clearColor` [obs].
- The hex helper `UI@0x37330` takes a 6-digit hex string. [unk] Alpha handling; I assume opaque.

Chrome colours (`MainWindow`: `UI@0x37d0` to `0x3cb0`; light apply `UI@0x10d80`; night apply
`UI@0x11110`) [obs]:

| Element | Light | Night |
|---|---|---|
| Window / content background | `NSColor.white` | `212226` |
| Window title text | `111111` | `5E5E61` |
| Top-bar button (default / pressed) | `333333` / `777777` | `5C5C61` / `E0E0EE` |
| Default button scheme (`defaultButtonColorScheme`) | `CCCCCC` | n/a |
| Bottom bar text, idle | `CCCCCC` | `555551` |
| Bottom bar text, active | [unk] | `F7F7F7` |
| Bottom bar highlighted background | `30A416` | same |
| Bottom bar hover background | [inf] `EEEEEE` | [inf] `060606` |
| Bottom bar click background | [unk] | `222222` |
| Calculator view background (night) | n/a | `18191C` (`UI@0x13840`) |
| Warning red | `FB3636` (`UI@0x15d40`) | |
| List selected mark | `06BE24` | |
| List window fill (light) | `E7E7E7` | [unk] |

List and filter colours (`UI@0x1d4f0`) [obs values, roles inferred]: light uses `BBBBBB` (icons),
`444444` (title text), `828282` and `EBEBEB` (borders), `DEDEDE` (hairlines). `676767`, `BFBFBF`,
`868585`, `3A3A3A`, `545454` and `888888` are the night variants. `ClickableTextField`
default/hover colours are `717171`/`000000` (light) and `848484`/`BFBFBF` (night) (`UI@0x25890`).

[unk] Assets.car colour sets and images. I could not decode `.car` files on Linux (`assetutil` is
macOS-only). The only image names found in code are `search` and `clear` (filter box icons).

---------------------------------------------------------------------------------------------

## 4. Result column

### 4.1 Layout
- Text area (`CalcTextView`, custom caret) on the left, results column on the right, with a
  vertical separator (`resultsSeparatorView`). Width 120 px by default (section 1.4).
- One `ResultView` per paragraph, held in `ResultViewController.resultViewArray`. Right-aligned
  text is [inf].
- `ResultView` properties [obs]: `text`, `prefix`, `font`, `padding`, `baselineY`,
  `overBackgroundPadding`, `overBackgroundRadius`, `showBaseline`, `showMidline`, `clipRect`, and the
  colours `textColor`, `overTextColor`, `overBackgroundColor`, `clickedBackgroundColor` (set from
  the theme's result, resultOver, resultOverBackground and resultClickedBackground).
- [unk] Pixel values for padding and radius.

### 4.2 Hover and click
- `mouseEntered`, `mouseExited` and `mouseMoved` on `ResultView` drive a rounded pill background
  (`overBackgroundColor`) with `overTextColor` text [obs].
- `mouseDown` shows `clickedBackgroundColor`; `mouseUp` calls the delegate's `resultViewClicked`
  (`UI@0x7400` to `0x7470`) [obs].
- The bottom-bar message field shows the localized hint "Click to copy" while the pointer is over
  a result (`calculatorViewResultViewEntered:`, `APP@0x100012c50`). On click it shows "Result
  copied" (`calculatorViewResultViewClicked:`, `APP@0x100012d50`). Both are Localizable.strings
  keys [obs].
- What a line click copies [inf]: the displayed result text (formatted, with precision and
  grouping applied) as a plain `NSString`, via `NSPasteboard.writeObjects` after `clearContents`
  (`UI@0x30f60`). [unk] The accessor behind the vtable +0x478 call; it could return a raw value.
- Copy Result (Cmd+Shift+C) copies the result text of the paragraph containing the caret
  (`UI@0x30d50`, `resultViewArray[paragraphIndex].text`) [obs].
- Copy All (Cmd+Opt+C): `String.stringForPasteboard(selectedRange:results:)` (`UI@0x192f0`) builds
  an `NSMutableString` of the selected paragraphs interleaved with their results [obs]. [unk] The
  text/result delimiter was not decoded; lines are joined with a newline.
- Rich type: `io.numi.unitvalue` (`UI@0x11a380`). `TextValuePasteboardItem` has two strings,
  `niceTextValue` (served for the plain-string type) and `parsableTextValue` (served for
  `io.numi.unitvalue`), assigned from the vtable slots [inf]. Paste prefers `io.numi.unitvalue`,
  otherwise plain text, which goes through `sanitizePasteboardString` (`UI@0x30b30`) [obs].
  Cmd+C in the text area puts a plain string built by `stringForPasteboard`.

### 4.3 Bottom bar and grand total
- `MainWindow` has `totalResultView` (a `ResultView`), `messageTextField`, `titleTextField`,
  `bottomAreaFont` and the bottom-bar colours from section 3.
- The label is "Total" (`Localizable.strings`: "Text before grand total at the bottom"). The
  value is `Context.grandTotalFormatted` from the engine (symbol at `APP@0x1000924f2`; JS name
  `grand_total_formatted`) [obs].
- Clicking the total (`windowGrandTotalClicked:`, `APP@0x1000126d0`) builds a
  `TextValuePasteboardItem`, with both nice and parsable strings set from the same accessor
  (`0x1a8` getter) [obs]. So both representations are the formatted grand total [inf].
- A click on the total's `ResultView` calls `MainWindow.resultViewClicked` (`UI@0x129b0`), which
  forwards to the same `windowGrandTotalClicked:` selector [obs].

### 4.4 Export and print
The export button pops up `exportMenu` with "Copy to Clipboard", "Export" and "Print" (from the
localization comments) [obs].
- Export writes a `txt` file (`APP@0x100030330` leads to `FUN_100032bb0`; `"txt"` literals at
  `APP@0x100005640` and nearby) [inf]. [unk] Whether results are included.
- "Copy to Clipboard" is `AppController.copyToClipboard:` [obs]. [unk] Content; plausibly the
  same string as Copy All.
- Print goes through `calculatorViewController` and uses the all-black print theme [inf].

---------------------------------------------------------------------------------------------

## 5. Multiple documents (NumiListWindow)

### 5.1 UI
- `NumiListWindow` (controller `NumiListWindowController`). Default size 230x210
  (`{{272,172},{230,210}}`, content inset 3 as `{{3,3},{224,204}}` inside a `CleanFrameView`)
  [obs: `NumiListWindow.nib`]. It is a floating mini panel that closes when it resigns main
  (`windowDidResignMain:`) [obs].
- Content: `FilterableListView`, made of a `FilterView` (search field with search icon and clear
  button) above a `NumiListView` (an `NSCollectionView` with flow layout) [obs]. When empty it
  shows "All your calculations will appear here"; the search placeholder is "Search" [obs].
- Each row (`NumiListCellView`) has a `label`, a `deleteButton` (revealed on hover [inf]) and a
  `selectedMarkView` (`06BE24`) [obs].
- Delegate callbacks [obs]: `itemSelected(at:)` and `itemDeleted(at:)`. Model operations:
  `reloadData`, `removeItem(at:)`, `insertItem(at:animated:)`, `updateItem(at:)`,
  `moveItem(_:to:)`. Filtering uses `filterText`, selection uses `selectedID` (a String).
- No evidence of rename or user drag-reorder. `moveItem(_:to:)` exists; it is probably used to
  move the most recently edited document to the top [inf]. The row label is likely the document's
  first line [inf].

### 5.2 Entry points
- Main-window buttons [obs]: "Add calculation", "All calculations", "Settings", "Export",
  "What's new", and a purchase button during trial (tooltips in `Localizable.strings`). Handlers:
  `windowAddButtonClicked:`, `windowListButtonClicked:sender:` (opens the list anchored to the
  button), `windowSettingsButtonClicked:` (calls the preferences `showWindow:`) and
  `windowExportButtonClicked:` (pops up the export menu).
- Menu: File > New (Cmd+N), List (Cmd+L), Import (Cmd+O), Export (Cmd+S), Print (Cmd+P),
  Close (Cmd+W). Edit > Delete All (Cmd+Opt+Backspace) [obs].
- Open and import: `.numi` and `.txt` files are read as UTF-8 into the calculator text view
  (`APP@0x100014420` tests the extension against `numi`) [obs]. The `numi://` scheme and the
  `fresh`, `noshadow` and demo launch arguments also exist [obs]; ignore them.

### 5.3 Persistence
- `StringStorageFileTracker` (main app) watches storage with an `NSMetadataQuery` whose predicate
  has the form `%K LIKE '*.<ext>'`. It handles `ubiquityIdentityDidChange` and `start(localOnly:)`
  (`APP@0x100016ec0`, `0x100015c60`, symbols) [obs]. So documents are plain files, one per
  calculation, in the iCloud ubiquity container or locally. The local path is an Application
  Support directory the app creates (error string "Could not create Application Support
  directory with error: %@") [obs].
- The extension is `txt` [inf] (`"txt"` literals near the storage code), with `.numi` accepted on
  open. [unk] The exact directory name; the `Ubiquity` literal hints at a `Documents` folder in
  the container. [unk] The file naming scheme (first line or UUID).
- The active document id and list state have no key in the 16-key defaults enum [obs]. [unk]
  Where the active document is remembered across launches.
- First launch with an empty store loads `sample.welcome`: `# Sample\n\n8 / (45 - 20%)\n5 inches
  in cm\n$9 in Euro` [obs]. A second sample, `sample.general`: `# My Calculations\n\nPrice: $6
  times 5\nFee: 8 CHF in Euro\nSum in USD - 9%\n\nNext friday + 2 weeks\n20 ml in tea spoons\n20%
  of what is 30 cm` [obs].
- The Demo menu (Welcome, General, Conversion, Sum, Dates, Variables, Functions and CSS samples,
  "Play ... Demo") is for marketing. Skip it [obs].

---------------------------------------------------------------------------------------------

## 6. Menu-bar mode, window, main menu

### 6.1 Menu-bar (status item) mode
- The `menuBarMode` pref (default false) sets `AppController.mode`. With it on, mode is 1 and
  `NSApp.setActivationPolicy(.accessory)`. With it off, mode is 0 and the policy is `.regular`
  (dock icon). Both switches run inside an `NSAnimationContext` grouping with delayed work.
  When leaving accessory mode, the app re-activates through `com.apple.dock` to restore focus
  (`APP@0x10002dee0`) [obs].
- `MenuBarController(image:menu:)` creates an `NSStatusItem` with a `StatusItemButton`
  (`initStatusItem` for 10.10+, `initStatusItem10` as a legacy path) [obs].
  - Left click calls handler(true) and `delegate.menuBarControllerStatusChanged(true)`, which
    shows or toggles the calculator window [inf] (`UI@0x23d0`).
  - Right click calls handler(false) and `popUpStatusItemMenu(menu)` (`UI@0x2475`).
- Status menu ("MenuBar Menu") items [obs, from localization comments]: Copy to Clipboard,
  Export, Import, Print, Preferences, Docs, Check for Updates, Quit. [unk] Order and separators.
  The icon is a template image set in `StatusItemButton.setImage:` / `drawRect:` (asset not
  decoded).

### 6.2 Window
- `MainWindow` has content view `AppWindowContentView` and a frameless card drawn by
  `CleanFrameView` (stroke, shadow, background). Default 370x360, centred; the frame is saved to
  `windowFrame` [obs].
- Always on top (`APP@0x10002d860`) [obs]: window level is `CGWindowLevelForKey(9)` (status
  window) when on, and `CGWindowLevelForKey(4)` (normal) when off. The preferences window gets
  the same level. One more panel uses level 3 (`APP@0x10002b250`).
- Theme also sets `NSWindow.appearance`: light forces `.aqua` (`UI@0x10d80`); night sets
  appearance to nil (`UI@0x11110`) [obs].
- `windowDidChangeOcclusionState` is handled [obs]; [unk] its effect.
- `window:willUseFullScreenPresentationOptions:` is overridden [obs]; [unk] which options.
- `ActivationWindow.nib` is licence entry only. Skip it.

### 6.3 Main menu (`MainMenu.nib`, app name "Numi")
Modifier masks [obs]: 1048576 = Cmd, 1572864 = Cmd+Opt, 1179648 = Cmd+Shift. An uppercase key
character also implies Shift.

| Menu | Item | Shortcut |
|---|---|---|
| Numi | About Numi | none |
| | Check for Updates / Deactivate (licence) | none |
| | Preferences… | Cmd+, |
| | Send Feedback | none |
| | Services; Hide Numi; Hide Others; Show All; Quit Numi | Cmd+H; Cmd+Opt+H; none; Cmd+Q |
| File | New | Cmd+N |
| | List | Cmd+L |
| | Import | Cmd+O |
| | Export | Cmd+S |
| | Print | Cmd+P |
| | Close | Cmd+W |
| Edit | Undo, Redo, Cut, Copy, Paste, Delete, Select All | Cmd+Z, Cmd+Shift+Z, Cmd+X, Cmd+C, Cmd+V, none, Cmd+A |
| | Copy Result | Cmd+Shift+C |
| | Copy All | Cmd+Opt+C |
| | Delete All | Cmd+Opt+Backspace |
| | Surround with Parentheses | Cmd+Shift+0 |
| | Find submenu: Find…, Find and Replace…, Find Next, Find Previous, Use Selection for Find, Jump to Selection | Cmd+F, Cmd+Opt+F, Cmd+G, Cmd+Shift+G, Cmd+E, Cmd+J |
| | Spelling and Grammar (Show Cmd+:, Check Now Cmd+;, check while typing, grammar, correct automatically), Substitutions (Smart Copy/Paste, Smart Dashes, Smart Links, Data Detectors, Text Replacement), Transformations (upper, lower, capitalize), Speech | standard |
| Demo | the sample and play items; Play Site Demo is Cmd+Opt+P | marketing, skip |
| Window | Minimize, Zoom, Bring All to Front | Cmd+M, none, none |
| Help | Docs (opens `https://numi.app/docs/`) | Cmd+? |

The custom menu items go to `AppController`: `copyToClipboard:`, `exportToFile:`,
`printContents:`, `preferences:`, `sendFeedback:`, `docs:`, `hotkey:` [obs].

---------------------------------------------------------------------------------------------

## 7. Findings that affect the clone

- Network: the app fetches `https://s3.numi.app/rates/currency.json` and
  `https://s3.numi.app/rates/coins2.json` (`APP@0x10001c000`) [obs]. I did not contact them. There
  is no currency-refresh preference [obs]. Refresh cadence is [unk], probably on launch or on a
  timer inside the engine.
- The default theme is dark: `nightTheme` is registered true [obs].
- A per-line result click copies formatted text. The grand total click also offers a parsable
  variant (probably identical text).
- Documents are one file per calculation, tracked with iCloud metadata. A Rust clone can use one
  directory of UTF-8 `.txt` files plus a small settings file.
- Colour sets and images in `.car` files are undecoded [unk].

---------------------------------------------------------------------------------------------

## 8. Evidence index

- Prefs nib: `Contents/Resources/Base.lproj/GeneralPreferences.nib`
- Main menu nib: `Contents/Resources/Base.lproj/MainMenu.nib`
- Defaults: register `APP@0x100007470`, key enum `APP@0x100007270`
- Hotkey: `APP@0x10002eb80`, `0x100008580`, `0x100027970`, `0x10000d030`
- Themes: `UI@0x24da0` (light), `UI@0x242d0` (night), `UI@0x247d0` (print), `UI@0x24570` (extension)
- Font metrics: `UI@0x2e940`, `UI@0x2a510`, `UI@0x2e860`
- Pasteboard: `UI@0xaac0`, `UI@0xacf0`, `UI@0x30a20`, `UI@0x30b30`, `UI@0x30d50`, `UI@0x30f60`
- Hover and click strings: `APP@0x100012c50`, `0x100012d50`; grand total `APP@0x1000126d0`
- Window level `APP@0x10002d860`; activation policy `APP@0x10002dee0`
- Status item: `UI@0x23d0`, `0x2475`
- Storage: `APP@0x100015c60`, `0x100016ec0`, `0x10002c870`
- List UI: `NumiUI.framework/Resources/NumiListWindow.nib`, `NumiListCellView.nib`,
  `FilterableListViewContents.nib`
