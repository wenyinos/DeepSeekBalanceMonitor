# Changelog

All notable changes to DeepSeek Balance Monitor are documented here.

## Rust v2.1.2 (2026-09-16)

### Added (five capabilities the 1.x build had)

- **The widget shows what the day has cost** (the other half of 4.5): the balance card carries a line
  under the balance with today's spending, which arrives over the contract's `today_spend` field
  (platform, currency, amount). The application's own alert reads the same figure

- **An alert for a day that is costing a lot** (4.5): when the day's DeepSeek spending passes a line
  the user sets (0 = off, and it ships off), the tray icon turns orange and says so once for that
  day. The line sits on the settings page beside the low-balance one; the icon gains a fifth state
  across every preset
- **The off-peak discount changing phase is announced** (4.4), once each way, against Beijing time
  (09:00–12:00 and 14:00–18:00 on weekdays are peak; the lunch break and the weekend are not). On by
  default, switchable
- **OpenCode Go's coarse windows are refined** (4.1): the endpoint gives weekly and monthly as whole
  percents and five-hourly as money, and the pools are known ($12 / $30 / $60) — so the money spent
  inside the current percent says where between two whole percents the truth is. The previous
  build's headline reading
- **MiniMax asks again when the answer was cut off** (4.2): the host drops a connection now and then
  (`UNEXPECTED_EOF`), and a second attempt a moment later answers. Three tries, a second apart, and
  a key the host rejects is not asked about again

### Fixed

- **DeepSeek's status page could not report a fault.** The address read here carried only
  FlashDuty's own `Open API` component, so every reading said "operational". The same system's
  `status.deepseek.com` carries DeepSeek's real components in the HTML
- The history table gains a `window` column (older databases get it added, defaulting to `monthly`,
  which is what every existing row is): the five-hour and weekly windows are recorded now, which is
  what the refinement reads
- **A refined percentage is shown as one.** `history::refined_percent` computes OpenCode Go's weekly
  and monthly windows down to decimals, and both places that draw them printed `{:.0}%` — the
  refinement arrived and was rounded away, so `70.43` had been `70` all along.
  `history::format_percent` leaves whole numbers whole and gives a refined one its two decimals
- **The first start after an upgrade no longer comes up without its keys.** Opening the database is a
  look-then-do — read the columns, add the one that is missing — and an upgrade is when two
  connections do it at once: the polling thread and the interface's own key lookup both add the
  `window` column (one is told it is already there) and take the write lock from each other
  (`database is locked`). The interface lost that race, so its lookup read no key at all and told
  the user to configure one, while the keys had been in the database the whole time — a restart
  found them. One connection at a time does the migrating now, and adding a column tolerates being
  told it is already there
- **A fresh installation can open its database again.** Adding the `window` column ran before the
  table it belongs to was created, so a database that did not exist yet was answered `no such
  table`. The tables are made first now, then the columns, then the indexes that name them

## Rust v2.1.1 (2026-09-15)

### Added

- **The desktop widget can start with the session too**: it has a start-up entry of its own
  (`widget_auto_start`, on by default), independent of the application's — so the widget can come up
  at login by itself (the application is not running yet, so it says so and reconnects every ten
  seconds), or wait to be started by the application. Closing the widget with its ✕ takes that
  entry away as well, or it would come back at the next login

### Fixed

- Creating the key file for the first time failed outright when another caller was creating the
  same file at that moment: Windows reports that as "access is denied" or "cannot find the file"
  rather than "already exists". All three answers are now treated as the same thing — look for the
  file again, waiting out a write that has not finished — and only when there is nothing to find is
  the original error reported. This is what failed the v2.1.0 Windows release three times in a row,
  always on the same test

## Rust v2.1.0 (2026-09-15)

### Added: the desktop widget, `dsmon2-widget`

- A program of its own: a frameless, translucent, always-on-top column of cards showing every
  platform's balance and subscription quota. It **reads from the application only** — a read-only
  interface on the loopback address, `127.0.0.1:18964` — and never calls a provider, opens the
  database or touches a key. With the application not running it says so, reconnects every ten
  seconds, and keeps the old readings on screen in grey
- Three kinds of card: **one per configured balance provider** (balance, burn rate and curve; a
  provider with no consumption history yet shows the balance and curve alone), one per
  subscription (each quota window on two lines: `name …… reset 3h 56m 86%` over a progress bar),
  and an **activity** card at the bottom (12 weeks × 7 days heat map, with tabs beside the title to
  switch between "all" — the configured subscriptions added up by date, which is what it opens on —
  and any single one; tabs use a one-word short name, with the full name on hover)
- Six title-bar buttons: light/dark, four opacity steps (25 / 50 / 75 / 90, shown as how full a
  circle is rather than as digits), keep-on-top (drawn in the accent colour while it is on),
  refresh now (greyed out while disconnected or while a poll is in flight), open the application's
  settings, close
- The window: **its width is fixed** (stretching a column of cards buys nothing) and **its height
  is dragged from the two bottom corners**; position and height are written back to the
  configuration and restored on the next start. The title bar does not drag the window — a drag
  area lying under the buttons swallows their clicks
- **A tray entry of its own**: "show/hide desktop widget", "start the app", "quit", with a left
  click toggling the panel
- **An icon of its own** (the application's whale with an accent-coloured corner badge), shared by
  the window and the tray; on X11 the window is a `Utility` and on Windows it is built with
  `with_taskbar(false)`, so neither a task bar nor a window list shows it
- Single-instance under names of its own (`…-widget` on D-Bus,
  `Local\DeepSeekBalanceMonitorWidget` on Windows): starting it twice only raises the window that
  is there, and neither program can mistake the other for itself
- Bilingual and light/dark through the same strings and palette as the application: change either
  on one side and the other follows within two seconds

### Added: the application side

- **A local read-only interface**: `GET /widget-status?days=1|7|30` and `GET /check` (which asks
  for a poll and answers with the current snapshot straight away). Loopback only, no credentials
  and no authentication; the contract is written down in `docs/INTERFACES.md`, which is all a
  Python build has to follow — the widget itself needs no change
- A tray entry, "Show desktop widget", whose tick is `widget_enabled`; the application starts the
  widget with it at login, and the widget's own close button turns that setting back off
- **One package per program**: a `.deb` and a `.rpm` for each on Linux, an `.msi` for each on
  Windows. The two MSIs install into one directory (`Program Files\DeepSeek Balance Monitor`),
  because the two executables have to be together to start each other — but they are separate
  products, installed, upgraded and removed on their own

### Changed

- `config.json` is written atomically (temporary file, then rename). Two processes read and write
  that file from this version on, and a half-written file would be taken for a corrupt one and
  moved aside — which is how a user's settings would disappear
- `widget_opacity` now has the four steps 0.25 / 0.50 / 0.75 / 0.90, replacing the two it had
  (0.5 and 1.0, the top one not being opaque) in earlier builds

### Fixed

- The widget's six title-bar buttons work again. The whole title bar used to double as the window's
  drag handle, and a drag area lying under the buttons takes the press for itself (the pointer
  moves at all, the window manager takes it over), so all six did nothing — the window could still
  be dragged, only the buttons were dead. Dragging now lives on three narrow bands along the left,
  right and bottom edges, and the title bar senses nothing
- Three things the contract asked for and the widget did not do (the serving side was complete all
  along — these were on the consuming end): the payload `version` is checked, so an application
  speaking a newer format shows a red strip ("the app is newer: update this widget") instead of
  drawing whichever half parses; the refresh button shows a poll in flight and goes dead while the
  application is away; and `widget_show_trend` is read at last — turning it off hides the curves
- Two gaps in the widget's Windows tray: a missing `use std::sync::Arc` (the Windows half is only
  compiled by CI, so this was found by reading), and a check for whether the shell really took the
  icon — the library does not report a refused registration, so a line now goes to the log

## Rust v2.0.2 (2026-09-14)

### Fixed

- The Windows tray icon is registered the ordinary way again. It had been given
  a fixed GUID so that a balloon could name it; that registration survives a
  killed process and the shell can then refuse to show the icon on the next run
- A click on the Windows tray icon raises its balloon again. A balloon names the
  icon by number, and the number used was off by one: the tray library numbers
  its icons from one and draws two numbers for each of ours, so the single icon
  this application makes is number two. A number the shell refuses is now
  followed by asking the shell which one it holds, and the balloon is raised again
- The window is hidden only while the tray can bring it back. A start with the
  session that reached the notification area before the shell did left the
  application running with nothing on screen, which is what "starting with the
  session does not work" looked like from the outside. The window now waits for
  the icon and stays on screen when the icon never comes, so a tray that never
  registered can no longer leave the application out of sight
- Starting with the session now reports itself: every start records whether the
  system holds the entry, since an entry that was never written and one the
  system will not run need different answers
- Importing the earlier database says where it looked when it finds nothing, and
  reads whichever of its tables are present: a database that was never given a
  key has no key table at all

### Changed

- The application keeps a data directory of its own (`dsmon2`: `%APPDATA%\dsmon2`
  on Windows, `~/.config/dsmon2` and `~/.local/state/dsmon2` on Linux). It used to
  share the earlier build's directory, where its own `config.json` and log were
  written over the earlier build's — and the earlier build's over its. What it
  left behind there is moved across on the first start, stored keys and history
  included, so nothing has to be entered again; the earlier build keeps its own
  database where it is
- "Start with the session" takes effect the moment it is ticked instead of waiting
  for "save", and the setting is written down at once: every start reconciles the
  entry against the configuration, so a switch that was never saved used to be
  written back out again at the next one

## Rust v2.0.1 (2026-09-14)

### Fixed

- Starting with the session now happens: the entry the desktop reads was only
  written when the setting changed, so a configuration that already said yes had
  a ticked box and nothing behind it. It is reconciled on every start, which also
  repairs an entry left by another build or one whose executable has moved
- Importing the earlier database works on Windows: it is read through a copy,
  since a read-only open of a WAL database needs the log files that the other
  version deletes when it closes. Keys the earlier Windows build protected with
  DPAPI are counted and reported instead of being carried over unreadable

### Changed

- The tray figure keeps a decimal below ten — 1.55 reads as 1.5 — and shows the
  whole part above it. Truncated rather than rounded either way, so the icon can
  never claim a larger balance than the account holds
- Windows notifications carry no mark: the shell's warning and information
  glyphs are louder than a balance reading
- The executables are `dsmon2` and `dsmon2.exe`, so they no longer share names
  with the 1.x build that installs beside them

## Rust v2.0.0 (2026-09-14)

The Python, macOS, Rainmeter, Plasma widget and CLI implementations are gone. 2.0 is one
Rust application with one user interface, shared by Windows and Linux.

### Added

- Desktop application: tray-resident, with a main window, sidebar navigation and a
  COSMIC-style visual system — six icon colour styles, each with a light and a dark scheme,
  switched from the button at the bottom of the sidebar
- Fourteen provider entries across eight families, in two kinds: accounts with a balance
  (DeepSeek, Kimi, StepFun, OpenRouter) and plans with quota windows (OpenCode Go, Command
  Code, MiniMax Token, MiniMax Coding, GLM Coding). MiniMax and GLM Coding are new
- Tray icon drawn with the balance and the state colour, republished as the reading changes;
  a click raises a summary of the balance, burn rate, service state and how stale the reading
  is, and the menu opens the window, polls now, opens settings or quits
- Closing the window puts it in the tray; the tray's quit entry is what ends the process
- Notifications for a low balance (per the alert mode), a service status that moved, a first
  run with nothing configured, and a database that had to be rebuilt
- Starting with the session, as a user setting: the `Run` key on Windows, `~/.config/autostart`
  on Linux — no service manager involved
- One instance at a time: a second launch raises the window of the copy already running
- Balance history per provider, a database size readout, and a cleanup that compacts the file
  rather than only deleting rows
- Import of keys and history from the 1.x database, which is read and never written, so both
  versions keep working side by side

### Changed

- Keys are encrypted with AES-256-GCM in the application itself; no DPAPI, libsecret or
  Keychain, and the same ciphertext format on both platforms
- Rust toolchain is stable, and the platform baseline is Windows 10 build 19041 and Linux
  kernel 6.1 distributions (Debian 12, Ubuntu 24.04, Fedora 38 and later)
- On a Wayland session the window opens through XWayland: Wayland lets an application neither
  hide its window nor bring it back, which is what closing to the tray needs
- Linux is delivered as `.deb` and `.rpm`; Windows as an MSI installer

### Removed

- The Python implementation, the macOS build and its WebView settings, the Rainmeter
  integration, the Plasma widget, the Linux CLI and its systemd unit
- The Rust 1.77.2 pin and the Windows 7 baseline that required it
- The multi-account model: one key per platform, entered on the settings page

## Rust v1.4.3 (2026-09-12)

### Fixed

- Both Rust implementations: Command Code monthly usage disappeared from the CLI, the Plasma widget, the Windows settings window and the Rainmeter `cc_monthly_*` fields once the API stopped returning `credits.planId`; the plan tier (and its monthly credit pool) is now inferred from the rolling window caps (5h/weekly), which identify the plan uniquely — Go 10 / GOAT 70 / Pro 80 / Max 10× 150 / Max 20× 300 / Team Pro 40 credits
- Both Rust implementations: monthly usage is `pool − credits.monthlyCredits`, clamped to the pool, so bonus credits can no longer produce a negative usage; plans whose window caps match no known tier (pay-as-you-go has no rolling windows) keep the monthly window unavailable

### Changed

- Command Code: `credits.planId` parsing and the fixed GOAT 70-credit constant are gone; the monthly window no longer requires the account to be identified as a GOAT plan

### Python implementation (no version bump in this release)

- Command Code monthly window uses the same window-cap inference instead of the removed `planId`, and the standard `command_code` platform entry shows the monthly window too; the `command_code_goat` entry gains `window_pools` (14/35/70) for refining
- Automatic polling no longer stops silently after saving settings: balance-check cycles re-arm themselves in a `finally` block, and settings save calls `restart_polling()` to cancel and re-arm atomically
- OCGo refined remaining uses round-band semantics (`|refined − raw| ≤ 0.5`, matching the API's rounded integer) instead of the previous floor band

## Rust v1.4.2 (2026-09-06)

### Fixed

- Rust Linux: an unrecognized service-status component ranked highest in `status_rank`, masking real `critical` outages — the CLI/widget showed "Status Unknown" instead of "Critical Outage"; unknown now ranks lowest, matching rust-windows
- Both Rust implementations: the busy-hour consumption-rate algorithm hardcoded a 10-minute check interval when deciding whether a gap is idle; it now uses the configured `interval_minutes`, so larger intervals (e.g. 60 min) no longer have their normal polling gaps sliced as idle time, which underestimated the rate
- Rust Windows: balance history rows are deduplicated with the same 120-second window as Rust Linux; previously every check inserted a row even with an unchanged balance, bloating the database (up to ~1440 rows/day per currency at a 1-minute interval)
- Rust Windows: a corrupted `config.json` no longer silently resets all settings — the file is backed up as `config.json.corrupt` and the failure is logged
- Rust Windows: the settings dialog validates all fields before persisting any API key; previously a new DeepSeek key was written to secure storage even when interval/threshold validation failed afterwards
- Rust Windows: export paths starting with `%USERPROFILE%` are expanded; the placeholder suggested it but the raw string was used, creating a literal `%USERPROFILE%` directory
- Both Rust implementations: OpenCode Go usage percent is clamped to 0–100 (the Linux CLI could print values above 100%)
- Both Rust implementations: balance display and the low-balance threshold prefer the CNY balance when multiple currencies exist, instead of the alphabetically-first currency (a USD balance could be compared against the CNY threshold)

### Changed

- Both Rust implementations: the SQLite database now runs in WAL journal mode with a 5-second busy timeout and indexes on `timestamp` and (`currency`, `timestamp`); concurrent access (Windows UI thread vs. balance-check / OpenCode Go / Command Code / Rainmeter threads, Linux daemon vs. widget-status) no longer fails with "database is locked", and history queries use the indexes
- Rust Windows: the tray icon font is loaded once per process (thread-local cache) instead of re-reading the font file on every tray refresh; unused `ensure_config_file` removed

### Security

- Rust Windows: launching a second copy is now blocked by a named mutex — the duplicate logs the failure, shows a native message box, and exits instead of running two tray icons racing on the same icon file and database
- Rust Windows: the local Rainmeter HTTP server no longer sends `Access-Control-Allow-Origin: *`, so web pages open in the user's browser can no longer read balance/subscription data or trigger checks via CORS (Rainmeter's WebParser does not rely on CORS)

## Rust v1.4.1 (2026-09-01)

### Changed

- Linux install split by scope: the `dsmon` binary and systemd user service stay system-level (`/usr/local/bin/dsmon`, `/etc/systemd/user/dsmon.service`, installed with sudo), while the Plasma widget and its icon install under the user directory (`~/.local/share/`) so widget updates never need sudo
- The Plasma widget calls `dsmon` by its absolute path `/usr/local/bin/dsmon` so it works from the Plasma `executable` engine regardless of the session PATH
- The installer runs `systemctl --user enable --now dsmon.service` after installation (for the sudo user), so the daemon is set to auto-start on login and starts immediately
- On non-systemd distributions (e.g. OpenRC), the installer skips the systemd service file and instead writes a desktop autostart entry (`~/.config/autostart/deepseek-balance-monitor.desktop`) that starts the daemon at desktop login
- The installer detects leftover user-level files from the earlier user-only install (1.4.1-rc: `~/.local/bin/dsmon`, `~/.config/systemd/user/dsmon.service`) and offers to remove them

## Rust v1.4.0 (2026-09-01)

### Added

- Command Code quota display (Rust Windows and Rust Linux): queries `api.commandcode.ai/alpha/billing/credits` (with `orgId` from `alpha/whoami`) and reports 5h / weekly / monthly usage. Monthly is derived for GOAT plans (70 credits) and left unavailable for other plans
- Windows: the settings dialog gains a "Subscriptions" tab holding both OpenCode Go and Command Code quota groups (each with three usage progress bars and a refresh button); all API keys (DeepSeek, OpenCode Go, Command Code) are entered on the Account tab
- Linux: new `dsmon command-code` (query quota), `dsmon command-code set-key <api_key>` (store the API key), and `dsmon command-code json` (JSON output) CLI commands
- Linux: the Plasma 6 widget gains a "Subscriptions" settings page showing OpenCode Go and Command Code quota, with credentials kept on the Account page; the widget main view adds a Command Code section with three usage progress bars
- Command Code API key is stored encrypted in the `secure_settings` table under the `command_code_api_key` key, never written to config.json
- Rust Windows: the local Rainmeter `/widget-status` interface now also exposes Command Code quota fields (`cc_configured`, `cc_error`, `cc_5h/weekly/monthly_percent` and `_line`), refreshed every 10 minutes by a background thread with last-good retention on failure — an interface reserve for the upcoming Rainmeter skin integration; the contract is documented in `rainmeter-widget/PYTHON_RAINMETER_INTEGRATION.md`

## Rust v1.3.3 (2026-08-30)

### Changed

- Rust Windows migrated from native-tls (Schannel) to rustls with embedded webpki-roots, matching rust-linux: no OS certificate store is consulted, Windows 7/8.1 installs validate out of the box, and TLS 1.3 is now available on old systems; TLS-inspecting proxies or security software will fail certificate validation since only the embedded root store is trusted
- Removed `scripts/update_windows_root_certs.bat`, unnecessary after the embedded-root migration (Py-Win requires Windows 10+ anyway); README TLS sections and directory trees updated accordingly

### Fixed

- Settings window font rendering: bold group titles no longer hardcode Segoe UI, whose missing CJK glyphs caused font fallback or tofu on the Chinese UI — the heading font now follows the unified UI family with real font enumeration (Microsoft YaHei UI, falling back to Microsoft YaHei / SimSun), and a stray 9-pixel size that rendered titles smaller than body text was dropped

## Rust v1.3.2 (2026-08-14)

### Changed

- Plasma 6 widget settings redesigned to match rust-windows: a new "Account" page holds both API keys (DeepSeek and OpenCode Go) plus the OpenCode quota bars; the General page is organised into Query / General / Proxy / Icon Appearance groups; the separate "OpenCode Go" settings page is merged into Account
- Plasma widget main view redesigned: the DeepSeek section follows a four-line layout (balance, last check, API status, estimated availability) with the refresh button moved to the top-right corner refreshing DeepSeek and OpenCode together; the OpenCode section shows three usage progress bars with larger type and bars
- Font hierarchy and spacing adjusted across both sections for a clearer visual order
- Rust Windows: the local Rainmeter `/widget-status` interface now includes OpenCode Go quota fields (`og_configured`, `og_error`, `og_rolling/weekly/monthly_percent` and `_line`), refreshed every 10 minutes by a background thread with last-good retention on failure; the interface contract and Python-port guidance are documented in `rainmeter-widget/PYTHON_RAINMETER_INTEGRATION.md`

## Rust v1.3.1 (2026-08-14)

### Changed

- Windows settings window redesigned: a new "Account" tab groups the DeepSeek and OpenCode Go API keys; the Settings tab is organised into Query / General / Proxy / Icon Appearance groups with bold titles and separator lines; all controls are aligned to a consistent grid with unified label and input columns

## Rust v1.3.0 (2026-08-14)

### Changed

- OpenCode Go quota now uses the official API (`opencode.ai/zen/go/v1/usage`) with a Bearer API key, replacing the workspace-dashboard scraper (workspace ID + auth cookie)
- Credentials simplified to a single API key, stored encrypted in the `secure_settings` table under `opencode_go_api_key`, never written to config.json
- Windows: the "OpenCode Go" settings tab now takes an API key instead of workspace ID / auth cookie
- Linux: `dsmon opencode-go set-key <api_key>` stores the API key; without an argument it reads from stdin, matching `dsmon set-key`
- Plasma: the "OpenCode Go" settings page gains API key input with save (same pattern as the DeepSeek key), and the quota progress bars use `QtControls.ProgressBar` for reliable rendering

## Rust v1.2.10 (2026-08-02)

### Added

- OpenCode Go quota display (Rust Windows and Rust Linux): queries the official `opencode.ai/zen/go/v1/usage` API with a Bearer API key and reports rolling (~5h), weekly, and monthly usage as used/remaining percentages with reset time
- Windows: the settings dialog gains an "OpenCode Go" tab with an API key input and a manual refresh button
- Linux: new `dsmon opencode-go` (query quota), `dsmon opencode-go set-key <api_key>` (store the API key), and `dsmon opencode-go json` (JSON output) CLI commands
- Linux: the Plasma 6 widget adds a dedicated "OpenCode Go" settings page showing quota, read directly from `dsmon opencode-go json`
- OpenCode Go API key is stored encrypted in the `secure_settings` table under the `opencode_go_api_key` key, never written to config.json

## Rust v1.2.6 (2026-06-08)

### Changed

- Consumption rate algorithm upgraded to busy-hour slicing (ported from Python v1.2.7): filters long idle gaps and flat periods; returns hourly rate instead of daily average
- Unified display format across all platforms:
  - Chinese: `📊 忙时消耗 0.06/小时 | 预计可用 28 天 4 小时`
  - English: `📊 Busy: 0.06/hr | Est. 28d 4h remaining`
- Updated `ConsumptionRate` struct: `daily_rate` → `hourly_rate`, `hours_left` → `busy_hours_left`
- Updated demo mode to use new rate fields
- Plasma widget updated to display hourly consumption rate

### Platform-Specific

- **Rust Windows**: Added `estimated_line` field to Rainmeter widget-status interface
- **Rust Linux**: Removed `estimated_line` (not needed for Plasma widget; uses `consumption_rate` field directly)

## Python v1.2.7 (2026-05-28)

### Fixed

- Fixed tkinter+pystray dual event-loop deadlock freezing the tray icon when settings/history dialogs were open

### Changed

- Consumption rate switched to busy-hour slicing algorithm: long idle gaps and flat periods are filtered out; displayed as hourly rate instead of daily average

## Python v1.2.6 (2026-05-13)

### Fixed

- Fixed connection refused and exit failure when system proxy (Clash etc.) goes down: empty `ProxyHandler` blocks system proxy, `socket.setdefaulttimeout` global fallback
- Fixed DNS resolution timeout blocking on network loss: global socket timeout + exit flag checks
- Fixed potential `cancel_timer` deadlock preventing `icon.stop()`: `icon.stop()` now runs before cleanup
- Removed API key `demo` trigger for dev mode: Python uses `--demo` CLI flag only

## Python v1.2.5 (2026-05-13)

### Added

- Developer Demo mode update: mock history data generated on startup; developer panel gains custom consumption rate & estimated hours display
- Custom icon colour live preview with hex validation on save
- History date filter with `YYYYMMDD` format query

### Changed

- History viewer extracted to `src/history_dialog.py`
- Tray notification and history page rate/time/prefix bilingual strings fully extracted to i18n keys

### Fixed

- Fixed settings "Enable proxy" off calling `install_proxy("")` with empty `ProxyHandler` overriding system proxy

## Rust v1.2.5 (2026-05-12)

### Added

- Standalone Plasma widget release asset: `deepseek-balance-monitor-*-plasmoid.plasmoid`
- Linux release tarballs now include the same Plasma widget package under `plasmoid/`
- Linux release assets include `checksums.txt` for tarball verification

### Changed

- Plasma widget display now follows the Rainmeter layout: balance line, relative last-check time, API service status, and estimated remaining time
- Plasma widget language settings now sync `cfg_language` back to `ui_language`, so English/Chinese selection survives Plasma restarts
- Low-balance display colour now takes priority over API-degraded colour, matching Rainmeter accent rules
- Rust Linux and Rust Windows service-status checks now use the FlashDuty-backed DeepSeek status page
- Consumption estimates use topped-balance history over a 7-day window, with retention-period fallback when needed
- Proxy settings now include an explicit enable toggle while preserving the proxy address when disabled

### Fixed

- Fixed Linux Plasma language changes appearing to reset after restarting `plasmashell`
- Fixed removed DeepSeek status REST API usage in Rust ports
- Fixed Windows settings title/footer behaviour to match the v1.2 settings design

## Python v1.2.2 (2026-05-12)

### Fixed

- Emergency migration of API service status monitoring to FlashDuty endpoint after DeepSeek replaced the underlying status page

## Python v1.2.1 (2026-05-12)

### Added

- Rainmeter local HTTP status interface on `127.0.0.1:17654`, auto-starts with the app, toggleable in settings
- Rainmeter `.rmskin` packaging script; CI auto-builds alongside EXE
- Rainmeter 2x high-DPI skin variants (ZH/EN)

### Changed

- API key storage unified to Fernet + SQLite, with legacy fallback; save_config() clears plaintext automatically
- Proxy now a checkbox toggle + address input; address is preserved when disabled
- Settings title simplified to `⚙️ Settings`, footer balance/last-check rows removed, version & contributor info shown
- Consumption rate restored to topped-balance with 7-day window and weighted average, plus retention fallback

## Rust v1.2 (2026-05-11)

### Added

- Rust Windows and Rust Linux versioned as `1.2.0`
- SQLite `secure_settings` encrypted API key storage (Rust Windows / Linux)
- Auto-migration from legacy plaintext `config.json.api_key` to encrypted storage
- Rust demo mode: save `demo` as the API key, data stored in isolated `demo_mode_balance` table
- Rust Linux `dsmon set-key` command for encrypted API key updates
- Rust Linux `dsmon set <field> <value>` command for single-field config updates
- Rust Linux installer prompts for API key on first launch when none is configured
- Rust Linux `uninstall.sh` script (preserves Plasma widget)
- Plasma 6 widget liquid-glass view with balance, last check, service status, estimated availability, refresh control, and emoji status text
- Rainmeter desktop widget via local `127.0.0.1:17654` interface; Rust Windows currently provides the interface
- GitHub Actions `.rmskin` packaging via `rmskin-builder`

### Changed

- Rust Linux daemon reloads config on each poll cycle; CLI changes take effect immediately
- Rust Linux CLI output is English-only, no desktop notifications
- Rust Windows opens settings dialog on first launch when no API key is configured
- Rust Windows/Linux separate `ui_language` (GUI) from `language` (CLI, fixed English)
- Rust CSV exports default to user home directory with date-suffixed filenames
- Rust demo data stays out of the real `balance_history` table
- Plasma widget settings use `dsmon set` command

## Python v1.2 (2026-05-11)

### Added

- Custom icon themes: 5 presets (Default / High Contrast / Bright / Dark Mode / Monochrome) + custom hex colours + icon stroke toggle
- History viewer: paginated table + trend chart + consumption rate analysis, with CSV export
- Consumption rate estimation: topped-balance weighted average, shown in balance notification and history viewer
- Demo mode: `--demo` flag with developer tools panel
- HTTP proxy support
- API key stored in Windows Credential Manager, config.json relegated to migration fallback
- MacOS WebView settings UI
- Unit test coverage for core API parsing and state transitions

### Changed

- Balance notification: emoji-prefixed lines, relative last-check time, service status repositioned
- API service status recorded alongside each balance history entry
- Settings, history, and dev tools share one Tk root window; history and dev tools support singleton raise-to-front
- Settings footer shows version, contributor credits, and project link
- MacOS build script adds DMG packaging

## Rust v1.1 (2026-05-10)

### Added

- Rust Windows native tray app, Win7+ support
- Rust Linux CLI + KDE Plasma 6 widget
- Rust history features: chart, days/currency filters, CSV export, `dsmon history` CLI
- Plasma widget daemon start/stop with command-error notifications
- Windows 7/8.1 root certificate update helper script

### Fixed

- Repaired Plasma widget configuration pages
- Added app icon to Rust Windows builds

## Python v1.1 (2026-05-10)

### Added

- API service status polling (`status.deepseek.com`); warm gray tray icon when degraded, independent status-change notifications
- "Top Up" tray menu item linking to `platform.deepseek.com/top_up`
- SQLite balance history storage with configurable log/record retention (default 30 days)
- Community port: Python MacOS app with Keychain encryption
- CONTRIBUTING.md for community porters
- GitHub Actions auto-build and attach EXE to releases

### Changed

- Low balance alerts: three modes (never / always / once per drop), default once
- Balance notification redesign: fixed title, inline breakdown, always-visible service status
- Settings validates numeric input ranges on save and warns on invalid values
- Replaced `requests` with stdlib `urllib.request`

## Rust v1.0.1 (2026-05-09)

Internal dev versions: Windows v0.1.0/v0.1.1, Linux v0.2.0

### Added

- Initial Rust Windows native build
- GitHub Actions Rust Windows release artifact workflow
- Rust Windows build documentation
- Merged Rust Windows port with upstream Python main
- Initial Rust Linux `dsmon` release build
- Linux packaging groundwork for command-line balance checks

### Fixed

- Hardened Rust Windows startup build behaviour
- Rust workflow tag trigger changed to `rust-v*` to avoid collision with Python tags
- Updated Rust port sync documentation

## Python v1.0.1 (2026-05-09)

### Changed

- Reorganized repository into `src/` and `scripts/`
- Deprecated currency selection (each account maps to a single fixed currency)
- Settings dialog behaviour improvements
- API key character encoding hardening
- Icon colour and alert toggle refinements
- README updates: direct download as recommended path, optimized preview images
- Code audit and formatting cleanup

## Python v1.0.0 (2026-05-06)

### Added

- Initial public Python Windows tray app release
- Periodic DeepSeek balance checks
- Low-balance alerts
- Settings dialog (API key, interval, threshold, language, auto-start)
- Tray icon rendering
- Windows executable build scripts
