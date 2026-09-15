# DeepSeek 余额监控 2.1

Windows 与 Linux 桌面应用：常驻系统托盘，替您盯着 DeepSeek 账户，以及其他它认识的平台。
一个纯 Rust 应用、一份界面代码，两个平台共用。

[English](README.md)

## 它做什么

- **读数始终在手边。** 托盘图标上就是余额，颜色随账户状态变化，悬停显示准确数字；单击弹出
  汇总——余额、忙时消耗速率、服务状态、上次查询距今多久。
- **多个平台一处看完。** 8 个平台家族、14 个条目，分两类：有余额的账户、有额度窗口的套餐。
- **历史留在本机。** 读数写入本地 SQLite，余额页与每张订阅卡片各自画图，可导出 CSV；每个
  平台保留自己的历史。
- **该提醒时才提醒。** 余额不足（可选仅一次 / 每次都提醒 / 不提醒）、服务状态变化、首次运行
  尚未配置、数据库被重建。
- **不打扰。** 关闭窗口是收进托盘，只有托盘的「退出」才结束进程；开机自启是一个勾选框；
  重复启动会唤出已在运行的窗口，而不是再开一个。
- **还有一块桌面小工具。** 单独的可执行文件 `dsmon2-widget`：无边框、半透明、可置顶的竖排
  卡片面板，挂在桌面上随时看。每个已配置的余额渠道一张卡（余额、消耗速率、曲线），每个订阅
  一张卡（各额度窗口与重置倒计时），最下方一张额度活动热力图（全部订阅合计，或点标签看某一家）。
  它**只从主程序取数**，自己不查 API、不读数据库、不碰密钥；主程序没在跑时会明确告诉您，并每
  10 秒自动重连。

## 支持平台

| 平台 | 条目 | 读数 |
|---|---|---|
| DeepSeek | `deepseek` | 余额、服务状态、忙时消耗速率 |
| OpenCode Go | `opencode_go` | 5h / 每周 / 每月额度 |
| Command Code | `command_code` | 5h / 每周 / 每月额度 |
| Kimi | `kimi_token_cn`、`kimi_token_global` | 余额 |
| StepFun | `stepfun_token_cn`、`stepfun_token_global` | 余额 |
| OpenRouter | `openrouter` | 余额 |
| MiniMax | `minimax_token_cn/global`、`minimax_coding_cn/global` | 5h / 每周额度 |
| GLM Coding | `glm_coding_cn`、`glm_coding_global` | 5h / 每周 / 每月额度 |

每个条目在设置页都有独立的密钥输入框；只有填了密钥的平台才会出现在侧边栏与订阅页。

## 系统要求

- **Windows** 10 build 19041（20H1）及以上，x64 或 arm64。
- **Linux** kernel 6.1 档及以上——Debian 12 / Ubuntu 24.04 / Fedora 38 起——amd64 或 arm64。
  Wayland 会话下需要 XWayland（原因见下），中文界面需要 CJK 字体。

## 安装

从 [Releases](https://github.com/wenyinos/DeepSeekBalanceMonitor/releases) 下载：

| 平台 | 安装包 |
|---|---|
| Linux | `.deb` / `.rpm`，amd64 与 arm64 |
| Windows | MSI 安装包，x64 与 arm64 |

主程序与**桌面小工具**各是一个包：可以只装主程序，也可以把小工具单独装上、单独升级。
小工具装好后由托盘菜单「显示/隐藏桌面小工具」开关，也可以从开始菜单直接打开。

包内已声明应用所需的依赖（XWayland、Vulkan、CJK 字体），正常安装会自动拉齐。

## 首次运行

打开设置页，为您使用的平台粘贴密钥即可。密钥落盘前用 AES-256-GCM 加密，密钥文件只有您本人
可读；**不会**写进 `config.json`，也不会离开本机。

界面中英双语，明暗默认跟随系统，切换按钮在侧边栏左下角。

## 数据位置

| 内容 | Linux | Windows |
|---|---|---|
| 配置 | `~/.config/dsmon2/config.json` | `%APPDATA%\dsmon2\config.json` |
| 历史与日志 | `~/.local/state/dsmon2/` | `%APPDATA%\dsmon2\` |
| 密钥 | `dsmon.db` 的 `secure_settings` 表，密钥文件在同一目录 | 同上 |

本版本用自己的目录（`dsmon2`），**绝不写入** 1.x 的 `~/.config/deepseek-balance-monitor`。
设置页可以把 1.x 的密钥与历史导入进来，那个库只会被读取，所以两个版本可以同时使用。
数据页还会显示数据库当前大小，并提供会压缩文件的手动清理（不是只删行）。

## 关于 XWayland

在 Wayland 会话下，应用通过 XWayland 打开窗口。Wayland 不允许应用隐藏自己的窗口、也不允许
自己把窗口唤回（两者都归合成器管），所以「关闭进托盘」会变成任务栏里一个唤不回来的窗口。
经 XWayland 则关闭就是真的收起来，托盘也能真的唤回。

`DSMON_NATIVE_WAYLAND=1` 可要求原生 Wayland 窗口；会话没有 X 显示时会自动回退到原生。

## 从源码构建

```bash
cargo test --workspace --locked
cargo build --release -p dsmon-ui --bin dsmon2 --bin dsmon2-widget   # 主程序 + 桌面小工具
cargo build --release -p dsmon-ui --bin dsmon2 --target aarch64-pc-windows-msvc
cargo run -p dsmon-ui --example widget_preview          # 开发时直接开小工具（不走托盘）
```

工具链为 stable。Linux 包在 Debian 12 容器里构建，以保持 glibc 2.36 基线：

```bash
packaging/build-packages.sh 2.1.1 arm64 target/release/dsmon2 dist
```

本版本的变化见 [CHANGELOG_zh.md](docs/CHANGELOG_zh.md)。
