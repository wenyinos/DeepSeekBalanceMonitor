# DeepSeek Balance Monitor / DeepSeek 余额监控

A Windows system tray application that periodically queries the DeepSeek API for account balance, displays it as a dynamic tray icon, and alerts on low balance.

一个 Windows 系统托盘应用，定时查询 DeepSeek API 账户余额，以动态图标形式显示在任务栏，余额过低时弹窗提醒。

---

## English

### Features

- **Dynamic Tray Icon** — Shows integer balance on a coloured rounded rectangle: teal when OK, red when low, gray when unknown. Values above 99 display as "OK", errors as "!".
- **Multi-Currency** — Supports CNY, USD, EUR, JPY, GBP, and 10+ other currencies from the API response. User selects a preferred currency; falls back to first available if not found.
- **Bilingual UI** — Chinese and English for all menus, dialogs, notifications, and tooltips. Switch from the settings window.
- **Configurable Interval** — Set from 1 to 1440 minutes (24 hours). Default: 10 minutes.
- **Low Balance Alert** — Desktop notification when balance drops below a user-defined threshold (default ¥1.00). Falls back to a text file if the notification API fails.
- **Settings Window** — tkinter dialog for API key, interval, threshold, preferred currency, language, and auto-start toggle.
- **Auto-Start** — Optional checkbox that registers the app in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- **High-DPI Aware** — Calls `SetProcessDpiAwareness(2)` before any GUI so everything renders sharp on high-resolution displays.
- **Single-File Build** — PyInstaller packages everything into one portable `.exe`.

### Requirements

- Windows 10 or later
- Python 3.10+ (for development only; end users just need the `.exe`)

### Quick Start

**Option A: Pre-Built EXE**

1. Download `DeepSeekBalanceMonitor.exe` from the Releases page.
2. Double-click to launch. On first run the settings window opens automatically — enter your DeepSeek API key.
3. The app lives in the system tray. Right-click the tray icon for the menu.

**Option B: Run from Source**

```bash
# 1. Install dependencies
pip install -r requirements.txt

# 2. Test your API key
python scripts\test_api.py YOUR_DEEPSEEK_API_KEY

# 3. Run the app
python main.py
```

**Verify Your API Key**

```bash
python scripts\test_api.py sk-xxxxxxxxxxxxxxxx
```

Prints your balance directly in the terminal. If it works here, it will work in the tray app.

### Building the EXE

```bash
# Install PyInstaller if not already present
pip install pyinstaller

# Run the build script
scripts\build_exe.bat
```

The build script:
1. Generates `app_icon.ico` (multi-resolution static icon)
2. Kills any running instance of the app
3. Runs PyInstaller with `--onefile --windowed --noconsole`
4. Auto-launches the built `dist\DeepSeekBalanceMonitor.exe`

### Project Structure

```
DeepSeekBalance/
├── src/
│   ├── __init__.py
│   ├── config.py             # Constants, i18n, logging, config I/O, DPI
│   ├── api_client.py         # fetch_balance() — DeepSeek API call
│   ├── icon_renderer.py      # create_icon_image() — dynamic tray icon
│   ├── app_state.py          # AppState class + registry helpers
│   ├── settings_dialog.py    # open_settings() — tkinter dialog
│   └── tray_app.py           # Check loop, notifications, menu, main()
├── main.py                   # Thin entry point
├── scripts/
│   ├── generate_icon.py      # Static multi-resolution .ico generator
│   ├── test_api.py           # Quick API key validation
│   ├── build_exe.bat         # One-click build + launch
│   ├── setup.bat             # pip install dependencies
│   └── run_silent.vbs        # Silent launcher
├── version_info.txt          # PyInstaller version resource
├── requirements.txt          # pystray, Pillow, requests
└── README.md
```

### Configuration

Settings are stored in `%APPDATA%\DeepSeek Balance Monitor\config.json`:

```json
{
  "api_key": "sk-xxxxxxxx",
  "interval_minutes": 10,
  "threshold_yuan": 1.0,
  "language": "zh",
  "preferred_currency": "CNY",
  "auto_start": false
}
```

Logs are written to `%APPDATA%\DeepSeek Balance Monitor\app.log`.

### Tray Menu

| Menu | Action |
|---|---|
| View Balance | Shows balance for all currencies in a notification |
| Check Now | Immediately queries the API and updates the icon |
| Settings | Opens the settings dialog |
| Quit | Stops the timer and exits |

### Icon Colours

| Colour | Meaning |
|---|---|
| Teal | Balance is above the alert threshold |
| Red | Balance is below threshold, or an API error occurred |
| Gray | First check not yet completed, or no API key configured |

### Planned

- **Mute / Snooze alerts** — "Remind later" / "Mute until balance recovers" options on low-balance notifications, so you are not pinged every interval while the balance stays under threshold.

### License

MIT

---

## 中文

### 功能

- **动态托盘图标** — 在圆角矩形底色上显示余额整数：青色表示正常，红色表示低于阈值，灰色表示未完成首次查询。超过 99 显示 "OK"，出错显示 "!"。
- **多币种支持** — 支持 CNY、USD、EUR、JPY、GBP 等 10 余种 API 返回的币种。用户可选择首选货币，若账户无此币种则自动回退到第一个可用币种。
- **双语界面** — 所有菜单、对话框、通知、提示均支持中文和英文，在设置窗口中切换。
- **可调查询间隔** — 1 至 1440 分钟（24 小时）自由设置，默认 10 分钟。
- **低余额提醒** — 余额低于用户设定阈值（默认 ¥1.00）时弹出桌面通知。通知 API 不可用时写入文本文件兜底。
- **设置窗口** — tkinter 对话框，用于配置 API Key、查询间隔、阈值、首选货币、语言、开机自启。
- **开机自启** — 可选勾选框，通过写入注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 实现。
- **高分屏适配** — 在任何 GUI 创建前调用 `SetProcessDpiAwareness(2)`，确保在高分辨率显示器上画质清晰。
- **单文件构建** — PyInstaller 将所有内容打包为一个便携 `.exe`。

### 运行要求

- Windows 10 及以上
- Python 3.10+（仅开发需要；最终用户只需 `.exe`）

### 快速开始

**方式 A：直接运行 EXE**

1. 从 Releases 页面下载 `DeepSeekBalanceMonitor.exe`。
2. 双击启动。首次运行会自动弹出设置窗口，输入 DeepSeek API Key。
3. 应用常驻系统托盘，右键托盘图标打开菜单。

**方式 B：源码运行**

```bash
# 1. 安装依赖
pip install -r requirements.txt

# 2. 测试 API Key
python scripts\test_api.py 你的DEEPSEEK_API_KEY

# 3. 运行
python main.py
```

**验证 API Key**

```bash
python scripts\test_api.py sk-xxxxxxxxxxxxxxxx
```

终端直接打印余额。这里能通则托盘应用也能通。

### 构建 EXE

```bash
# 先安装 PyInstaller
pip install pyinstaller

# 运行构建脚本
scripts\build_exe.bat
```

构建流程：
1. 生成 `app_icon.ico`（多分辨率静态图标）
2. 终止已运行的应用实例
3. 运行 PyInstaller，参数 `--onefile --windowed --noconsole`
4. 自动启动构建产物 `dist\DeepSeekBalanceMonitor.exe`

### 项目结构

```
DeepSeekBalance/
├── src/
│   ├── __init__.py
│   ├── config.py             # 常量、双语字典、日志、配置读写、DPI 感知
│   ├── api_client.py         # fetch_balance() — 单次 API 余额查询
│   ├── icon_renderer.py      # create_icon_image() — 动态托盘图标渲染
│   ├── app_state.py          # AppState 类 + 注册表辅助函数
│   ├── settings_dialog.py    # open_settings() — tkinter 设置对话框
│   └── tray_app.py           # 查询循环、通知、菜单回调、main()
├── main.py                   # 薄入口
├── scripts/
│   ├── generate_icon.py      # 静态多分辨率 .ico 生成器
│   ├── test_api.py           # API Key 快速验证脚本
│   ├── build_exe.bat         # 一键构建 + 启动
│   ├── setup.bat             # pip 安装依赖
│   └── run_silent.vbs        # 静默启动脚本
├── version_info.txt          # PyInstaller 版本资源（通知显示名称）
├── requirements.txt          # pystray, Pillow, requests
└── README.md
```

### 配置

配置文件路径：`%APPDATA%\DeepSeek Balance Monitor\config.json`

```json
{
  "api_key": "sk-xxxxxxxx",
  "interval_minutes": 10,
  "threshold_yuan": 1.0,
  "language": "zh",
  "preferred_currency": "CNY",
  "auto_start": false
}
```

日志路径：`%APPDATA%\DeepSeek Balance Monitor\app.log`

### 托盘菜单

| 菜单 | 作用 |
|---|---|
| 查看余额 | 弹窗显示所有币种余额 |
| 立即查询 | 立即查询 API 并刷新图标 |
| 设置 | 打开设置窗口 |
| 退出 | 停止定时器并退出 |

### 图标颜色

| 颜色 | 含义 |
|---|---|
| 青色 | 余额高于预警阈值 |
| 红色 | 余额低于阈值，或 API 查询出错 |
| 灰色 | 尚未完成首次查询，或未配置 Key |

### 计划更新

- **预警免打扰** — 低余额通知中增加「稍后提醒」与「余额恢复前不再提醒」选项，避免余额持续低于阈值时每个间隔都弹窗。

### 协议

MIT
