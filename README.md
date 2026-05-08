# DeepSeek Balance Monitor / DeepSeek 余额监控

A Windows system tray application that periodically queries the DeepSeek API for account balance, displays it as a dynamic tray icon, and alerts on low balance.

一个 Windows 系统托盘应用，定时查询 DeepSeek API 账户余额，以动态图标形式显示在任务栏，余额过低时弹窗提醒。

---

## English

### Features

- **Dynamic Tray Icon** — Shows integer balance on a coloured rounded rectangle: teal when OK, red when low, gray when unknown. Values above 99 display as "OK", errors as "!".
- **Currency Display** — Shows the actual currency returned by the API alongside each balance.
- **Bilingual UI** — Chinese and English for all menus, dialogs, notifications, and tooltips. Switch from the settings window.
- **Configurable Interval** — Set from 1 to 1440 minutes (24 hours). Default: 10 minutes.
- **Low Balance Alert** — Desktop notification when balance drops below a user-defined threshold (default 1.00). Can be disabled independently; the tray icon still turns red as a visual warning.
- **Settings Window** — Configure API key, interval, threshold, language, auto-start, and alert toggle.
- **Auto-Start** — Optional: register to launch on Windows boot.
- **High-DPI Aware** — Renders sharp on high-resolution displays.

### Requirements

- Windows 10 or later
- Python 3.10+

### Run from Source

```bash
pip install -r requirements.txt
python main.py
```

On first launch the settings window opens automatically — enter your DeepSeek API key. The app lives in the system tray; left-click the icon to view balance, right-click for the menu.

### Building the EXE

```bash
pip install pyinstaller
scripts\build_exe.bat
```

The script generates the static icon, builds a single-file `dist\DeepSeekBalanceMonitor.exe`, and launches it.

### Project Structure

```
DeepSeekBalance/
├── src/                       # Application package
│   ├── config.py
│   ├── api_client.py
│   ├── icon_renderer.py
│   ├── app_state.py
│   ├── settings_dialog.py
│   └── tray_app.py
├── scripts/                   # Build & utility scripts
│   ├── generate_icon.py
│   ├── test_api.py
│   ├── build_exe.bat
│   ├── setup.bat
│   └── run_silent.vbs
├── main.py
├── requirements.txt
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

  "auto_start": false,
  "enable_alerts": true
}
```

Logs are written to `%APPDATA%\DeepSeek Balance Monitor\app.log`.

### Tray Menu

| Action | Trigger |
|---|---|
| View Balance | Left-click the icon, or Right-click → View Balance |
| Check Now | Right-click → Check Now |
| Settings | Right-click → Settings |
| Quit | Right-click → Quit |

### Icon Colours

| Colour | Meaning |
|---|---|
| Teal | Balance is above the alert threshold |
| Red | Balance is below threshold, or an API error occurred |
| Gray | First check not yet completed, or no API key configured |

### License

MIT

---

## 中文

### 功能

- **动态托盘图标** — 在圆角矩形底色上显示余额整数：青色表示正常，红色表示低于阈值，灰色表示未完成首次查询。超过 99 显示 "OK"，出错显示 "!"。
- **货币显示** — 余额标注实际币种，查出来是什么就显示什么。
- **双语界面** — 所有菜单、对话框、通知、提示均支持中文和英文，在设置窗口中切换。
- **可调查询间隔** — 1 至 1440 分钟（24 小时）自由设置，默认 10 分钟。
- **低余额提醒** — 余额低于设定阈值（默认 1.00）时弹出桌面通知。提醒可独立关闭，关闭后托盘图标仍会变红作为视觉预警。
- **设置窗口** — 配置 API Key、查询间隔、阈值、语言、开机自启、提醒开关。
- **开机自启** — 可选：注册到 Windows 启动项。
- **高分屏适配** — 在高分辨率显示器上画质清晰。

### 运行要求

- Windows 10 及以上
- Python 3.10+

### 源码运行

```bash
pip install -r requirements.txt
python main.py
```

首次运行会自动弹出设置窗口，输入 DeepSeek API Key。应用常驻系统托盘，左键单击图标查看余额，右键打开菜单。

### 构建 EXE

```bash
pip install pyinstaller
scripts\build_exe.bat
```

脚本会生成静态图标、构建单文件 `dist\DeepSeekBalanceMonitor.exe` 并自动启动。

### 项目结构

```
DeepSeekBalance/
├── src/                       # 应用主包
│   ├── config.py
│   ├── api_client.py
│   ├── icon_renderer.py
│   ├── app_state.py
│   ├── settings_dialog.py
│   └── tray_app.py
├── scripts/                   # 构建与工具脚本
│   ├── generate_icon.py
│   ├── test_api.py
│   ├── build_exe.bat
│   ├── setup.bat
│   └── run_silent.vbs
├── main.py
├── requirements.txt
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

  "auto_start": false,
  "enable_alerts": true
}
```

日志路径：`%APPDATA%\DeepSeek Balance Monitor\app.log`

### 托盘菜单

| 操作 | 方式 |
|---|---|
| 查看余额 | 左键单击图标，或右键 → 查看余额 |
| 立即查询 | 右键 → 立即查询 |
| 设置 | 右键 → 设置 |
| 退出 | 右键 → 退出 |

### 图标颜色

| 颜色 | 含义 |
|---|---|
| 青色 | 余额高于预警阈值 |
| 红色 | 余额低于阈值，或 API 查询出错 |
| 灰色 | 尚未完成首次查询，或未配置 Key |

### 协议

MIT
