# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

跨平台 DeepSeek API 余额监控工具，支持 Windows、Linux、macOS。包含 Python 和 Rust 两种实现，以及 Rainmeter (Windows) 和 Plasma 6 (Linux) 桌面小组件。

## 常用命令

### Python (Windows/macOS)

```bash
# 安装依赖
pip install -r requirements.txt

# 运行应用
python main.py

# 运行测试
python -m unittest discover tests

# 构建 Windows 可执行文件
scripts\build_exe.bat

# 构建 macOS 应用
cd src/mac && pip install -r requirements.txt && bash ../scripts/build_mac.sh
```

### Rust Linux

```bash
cd rust-linux
cargo build --release
sudo ./install.sh  # 安装到系统
```

### Rust Windows

```bash
cd rust-windows
rustup toolchain install 1.77.2-x86_64-pc-windows-msvc
cargo +1.77.2 build --release --target x86_64-pc-windows-msvc --locked
```

### Linux CLI 命令

```bash
dsmon check          # 查询余额
dsmon daemon         # 守护进程模式
dsmon set-key        # 设置 API Key
dsmon set <field> <value>  # 修改配置
dsmon history [days] # 查看历史
dsmon widget-status  # 输出 Plasma 小组件 JSON
```

## 高层架构

### 多平台实现

- **Python Windows** (`src/`, `main.py`): pystray + Tkinter，Windows 10+
- **Python macOS** (`src/mac/`): rumps + pywebview，macOS 10.14+
- **Rust Windows** (`rust-windows/`): native-windows-gui，Windows 7+
- **Rust Linux** (`rust-linux/`): CLI + Plasma 6 小组件，RHEL 8/Ubuntu 20.04+

### Python 核心模块

- `src/config.py`: 常量、i18n (`_T` 字典)、配置加载/保存、DPI 感知
- `src/api_client.py`: DeepSeek API 余额查询、服务状态检查、代理配置
- `src/tray_app.py`: 托盘应用主循环、通知、菜单、余额检查调度
- `src/app_state.py`: 共享状态管理（余额、配置、运行状态）
- `src/icon_renderer.py`: 动态托盘图标生成（Pillow）
- `src/settings_dialog.py`: 设置窗口 UI
- `src/history_dialog.py`: 历史记录查看器（分页、图表、CSV 导出）
- `src/storage.py`: SQLite 数据库操作、忙时切片消耗速率算法
- `src/secure_settings.py`: Fernet + SQLite 加密存储 API Key
- `src/credential_store.py`: Windows 凭据管理器（已过时，仅作回退）
- `src/rainmeter_server.py`: Rainmeter 本地 HTTP 接口 (127.0.0.1:17654)

### Rust 实现

- `rust-linux/src/main.rs`: Linux CLI 和守护进程，含 Plasma 小组件支持
- `rust-windows/src/main.rs`: Windows 原生 GUI 应用
- `rust-linux/src/demo.rs`: 演示模式数据生成

## 核心约定

### 配置存储

- Windows: `%APPDATA%\DeepSeek Balance Monitor\config.json`
- Linux: `~/.config/deepseek-balance-monitor/config.json`
- macOS: `~/Library/Application Support/DeepSeek Balance Monitor/config.json`
- API Key 不存储在 config.json，使用加密存储（SQLite + Fernet/DPAPI/Keychain）

### i18n

- Python 版所有 UI 文案在 `src/config.py` 的 `_T` 字典中管理
- 新增 UI 文案必须添加到 `_T`，不要硬编码
- CLI 输出固定为英文，不随 `ui_language` 切换

### 消耗速率算法 (v1.2.7+)

忙时切片算法替代原有简单充值跳变检测：
- 识别忙时区间（排除闲时/平直段）
- 返回忙时小时消耗速率（CNY/小时）和预计可用忙时小时数
- UI 层负责将小时数换算为天/小时格式展示
- 移植版本调用 `get_consumption_rate()` 时注意返回值语义已变更（hourly_rate 而非 daily_rate）
- **Rust 版本已跟进**：Rust Windows 和 Rust Linux 均已实现忙时切片算法
- **显示格式统一**：
  - 中文：`📊 忙时消耗 0.06/小时 | 预计可用 28 天 4 小时`
  - 英文：`📊 Busy: 0.06/hr | Est. 28d 4h remaining`
- **Rainmeter 小组件**（仅 Windows）：通过 `estimated_line` 字段获取格式化文本
- **Plasma 小组件**（仅 Linux）：直接从 `consumption_rate` 字段计算显示

### 图标颜色状态

| 颜色 | 含义 |
|---|---|
| Teal | 余额正常 |
| Red | 余额不足或 API 错误 |
| Warm Gray | API 服务降级 |
| Gray | 尚未查询或未配置 Key |

### API 端点

- `api.deepseek.com/user/balance` — 余额查询
- `status.flashcat.cloud/deepseek` — FlashDuty 服务状态（RSC 解析）

### 代理支持

- `http_proxy` 配置项 + `proxy_enabled` 开关
- 禁用时保留代理地址不清除
- Python 版修改后即时生效，Rust 版在下次轮询时生效

## 测试

Python 测试位于 `tests/test_core.py`，使用 unittest。运行：

```bash
python -m unittest discover tests
```

涉及 API 的改动，使用 `python scripts/test_api.py YOUR_API_KEY` 验证连接。

## 注意事项

- Rust 构建需要 1.77.2 toolchain，Windows 需要 MSVC target
- Windows 7/8.1 的根证书可能需要更新（运行 `scripts\update_windows_root_certs.bat`）
- Rainmeter 小组件在 127.0.0.1:17654 提供本地状态接口
- Plasma 小组件需要 KDE Plasma 6，通过 `dsmon widget-status` 获取数据
- 修改 API 客户端时需同步检查 Python 和 Rust 实现
- Python 版使用 tkinter + pystray 双事件循环，注意避免死锁
