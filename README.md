# DeepSeek Balance Monitor 2.0

A desktop application for Windows and Linux that keeps an eye on your DeepSeek account — and on
the other providers it knows — from the system tray. One Rust application, one user interface,
both platforms.

[中文版](README_zh.md)

## What it does

- **The reading is always in sight.** The tray icon carries the balance and turns colour with
  the state of the account; the tooltip spells out the figure. A click raises a summary: the
  balance, the burn rate, the service state, and how long ago the reading was taken.
- **Every provider in one window.** Fourteen entries across eight families, in two kinds:
  accounts with a balance, and plans with quota windows.
- **History that stays on your machine.** Readings go into a local SQLite database, charted on
  the balance page and on each subscription card, and exportable to CSV. Every provider keeps
  its own history.
- **Alerts when they are worth raising.** A low balance (once, every time, or never), a service
  status that moved, a first run with nothing configured, a database that had to be rebuilt.
- **Out of the way.** Closing the window leaves it in the tray, and only the tray's quit entry
  ends the process. Starting with the session is a checkbox. Launching a second copy raises the
  window of the one already running instead of starting another.

## Providers

| Provider | Entries | Reading |
|---|---|---|
| DeepSeek | `deepseek` | balance, service status, burn rate |
| OpenCode Go | `opencode_go` | 5h / weekly / monthly quota |
| Command Code | `command_code` | 5h / weekly / monthly quota |
| Kimi | `kimi_token_cn`, `kimi_token_global` | balance |
| StepFun | `stepfun_token_cn`, `stepfun_token_global` | balance |
| OpenRouter | `openrouter` | balance |
| MiniMax | `minimax_token_cn/global`, `minimax_coding_cn/global` | 5h / weekly quota |
| GLM Coding | `glm_coding_cn`, `glm_coding_global` | 5h / weekly / monthly quota |

Every entry has its own key field on the settings page. Only the ones you fill in appear in the
sidebar and on the subscription page.

## Requirements

- **Windows** 10 build 19041 (20H1) or later, x64 or arm64.
- **Linux** kernel 6.1 era or later — Debian 12, Ubuntu 24.04, Fedora 38 and up — amd64 or
  arm64. On a Wayland session XWayland is required (see below), and the Chinese interface
  needs a CJK font.

## Install

Download from [Releases](https://github.com/wenyinos/DeepSeekBalanceMonitor/releases):

| Platform | Packages |
|---|---|
| Linux | `.deb` / `.rpm`, amd64 and arm64 |
| Windows | MSI installer, x64 and arm64 |

The packages declare what the application needs — XWayland, Vulkan, a CJK font — so a normal
install pulls them in.

## First run

Open the settings page and paste a key for whichever provider you use. Keys are encrypted with
AES-256-GCM before they touch the disk, with a key file only your user can read; they are never
written to `config.json` and never leave the machine.

The interface is bilingual (Chinese and English), the scheme follows the desktop by default,
and the day/night switch sits at the bottom of the sidebar.

## Where things are kept

| What | Linux | Windows |
|---|---|---|
| Configuration | `~/.config/deepseek-balance-monitor/config.json` | `%APPDATA%\DeepSeek Balance Monitor\config.json` |
| History and log | `~/.local/state/deepseek-balance-monitor/` | `%APPDATA%\DeepSeek Balance Monitor\` |
| Keys | `secure_settings` table in `dsmon.db`, key file beside it | same |

The settings page can import keys and history from the 1.x build. That database is only ever
read, so both versions keep working side by side. The data page also shows the size of the
database and offers a cleanup that compacts the file rather than only deleting rows.

## About XWayland

On a Wayland session the window opens through XWayland. Wayland does not let an application
hide its own window or bring it back — the compositor owns both — so closing to the tray would
leave a window in the task bar that could not be called back. Through XWayland a close really
does put the window away, and the tray really does bring it back.

`DSMON_NATIVE_WAYLAND=1` asks for a native Wayland window instead; a session with no X display
falls back to it on its own.

## Build from source

```bash
cargo test --workspace --locked
cargo build --release -p dsmon-ui --bin dsmon           # Linux: target/release/dsmon
cargo build --release -p dsmon-ui --bin deepseek-balance-monitor \
  --target aarch64-pc-windows-msvc
```

Toolchain: stable. The Linux packages are built in a Debian 12 container so the binary keeps a
glibc 2.36 floor:

```bash
packaging/build-packages.sh 2.0.0 arm64 target/release/dsmon dist
```

See [CHANGELOG.md](CHANGELOG.md) for what changed in this version.
