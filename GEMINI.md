# DeepSeek Balance Monitor 项目指南

这是一个跨平台的 DeepSeek API 余额监控工具，支持 Windows、Linux 和 MacOS。项目包含 Python 和 Rust 两种实现，以及桌面插件（Rainmeter 和 Plasma Widget）。

## 项目概览

- **功能**：定期查询 DeepSeek API 余额，并在余额低于阈值时发出通知。托盘图标实时显示余额状态。
- **技术栈**：
  - **Python (Windows/Mac)**：使用 `pystray` (Win) 或 `rumps` (Mac) 处理托盘，`Tkinter` (Win) 或 `pywebview` (Mac) 处理 UI。
  - **Rust (Windows/Linux)**：原生实现，追求性能和稳定性。Linux 版包含 CLI 和 KDE Plasma 6 挂件。
  - **数据存储**：使用 SQLite (`balance_history.db`) 存储历史记录和加密配置。
  - **安全性**：API Key 通过加密存储（Windows 使用 DPAPI/Fernet，Linux 使用 AES-256-GCM，Mac 使用 Keychain）。

## 目录结构

- `main.py`: Python Windows 版入口。
- `src/`: Python 核心逻辑（API 客户端、图标渲染、配置管理、托盘逻辑等）。
- `src/mac/`: MacOS 专用 Python 代码。
- `rust-linux/`: Rust Linux CLI 及 Plasma 6 挂件源码。
- `rust-windows/`: Rust Windows 原生应用源码。
- `rainmeter-widget/`: Rainmeter 桌面皮肤源码。
- `scripts/`: 构建和辅助脚本。
- `assets/`: 图标、字体及预览图。
- `tests/`: Python 单元测试。

## 开发与构建

### Python 环境 (Windows/Mac)

1.  **安装依赖**：
    ```bash
    pip install -r requirements.txt
    ```
2.  **运行**：
    ```bash
    python main.py
    ```
3.  **构建执行文件**：
    - Windows: 执行 `scripts\build_exe.bat`。
    - Mac: 执行 `scripts/build_mac.sh`。
4.  **测试**：
    ```bash
    python -m unittest discover tests
    ```

### Rust 环境 (Linux/Windows)

1.  **Linux 构建**：
    ```bash
    cd rust-linux
    cargo build --release
    ```
    - 安装：`sudo ./install.sh`
    - CLI 命令：`dsmon check` (查余额), `dsmon daemon` (守护进程), `dsmon set-key` (设密钥)。
2.  **Windows 构建**：
    ```bash
    cd rust-windows
    cargo build --release
    ```

## 核心约定

- **国际化 (i18n)**：
  - Python 版在 `src/config.py` 的 `_T` 表中管理翻译。
  - Rust 版在源码中处理，部分 UI 字符串通过 `ui_language` 配置切换。
- **配置管理**：
  - Windows: `%APPDATA%\DeepSeek Balance Monitor\config.json`
  - Linux: `~/.config/deepseek-balance-monitor/config.json`
  - 注意：API Key 不直接存储在 `config.json` 中，而是通过加密手段存储。
- **图标渲染**：
  - Python 使用 `Pillow` 动态生成托盘图标。
  - 不同状态对应不同颜色：Teal (正常), Red (低余额/错误), Warm Gray (服务降级), Gray (无数据)。
- **代码风格**：
  - 遵循 PEP 8 (Python) 和 Rust 官方规范。
  - 核心逻辑（如 API 查询、余额计算）在 Python 和 Rust 版本间应保持一致性。

## 贡献建议

- 修改 Python GUI 逻辑时，需注意 DPI 感知（已在 `src/config.py` 中初始化）。
- 修改 API 客户端时，需同步检查 Python (`src/api_client.py`) 和 Rust (`rust-linux/src/main.rs` 等) 的实现。
- 提交前请运行单元测试。
