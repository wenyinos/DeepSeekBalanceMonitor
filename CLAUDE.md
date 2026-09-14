# CLAUDE.md

本文件是项目的权威细节来源（架构、数据格式、平台集成、约定）。面向使用者的说明见
`README.md`；agent 容易踩坑的高信号事实见 `AGENTS.md`；实施计划见 `PLAN.md`；
各订阅平台的接口细节见 `PLATFORM_PORTING.md`；1.x 差异与未移植功能见 `GAPS.md`。

## 项目概述

跨平台桌面应用（Windows + Linux，**同一份界面代码**）：常驻托盘，定时查询 DeepSeek 及其他
平台的余额与套餐额度，写入本地历史，余额低或服务异常时发系统通知。版本 2.0.0，纯 Rust，
无 WebView、无 Python。

## 常用命令

```bash
cargo test --workspace --locked         # 全部测试（与 CI 一致）
cargo test -p dsmon-core                # 只跑后端
cargo fmt --all --check                 # CI 校验格式
cargo build --release -p dsmon          # Linux 可执行文件：target/release/dsmon
cargo run -p dsmon-ui --example preview # 开发时直接启动界面（不走平台入口）

# 打包（需要 dpkg-deb / rpmbuild / ImageMagick，CI 在容器里跑）
packaging/build-packages.sh 2.0.0 target/release/dsmon dist
```

Rust 工具链为 **stable**（根 `rust-toolchain.toml`）。1.x 的 1.77.2 固定版本随 Windows 7
支持一起取消，本版平台基线是 Windows 10 build 19041 与 kernel 6.1 档发行版。

## 架构

```
Cargo.toml                 # workspace：四个成员，共用 toolchain 与锁文件
dsmon-core/                # 平台无关，无 GUI 依赖
  catalog.rs               # 14 个平台条目（key/显示名/payg|package/窗口清单/是否已实现）
  config.rs                # AppConfig（config.json）
  paths.rs                 # 配置与状态目录（Windows %APPDATA% / Linux XDG 分离）
  crypto.rs                # AES-256-GCM（ring），密文 DSBM1 + nonce + 标签
  storage.rs               # SQLite：建表、历史读写、去重、裁剪、清理、旧库导入
  history.rs               # 忙时消耗速率、每日用量、CSV 导出
  model.rs                 # 余额、额度窗口（QuotaWindow / PackageQuota）、API 响应结构
  monitor.rs               # 后台轮询线程 + Snapshot（界面读取的唯一数据源）
  platforms/               # 各平台客户端：deepseek, opencode_go, command_code, kimi,
                           # stepfun, openrouter, minimax, glm, status（服务状态页）
  icon.rs                  # 托盘图标位图渲染 + 应用图标解码
  autostart.rs             # 开机自启：Windows 注册表 Run / Linux ~/.config/autostart
  demo.rs                  # 演示模式（API Key 填 demo 触发）
dsmon-ui/                  # 唯一一份界面
  app.rs                   # 应用外壳：窗口、侧边栏、页面分发、logic/ui 回调
  theme.rs                 # 6 套图标配色 × 日/夜双主题，语义色与 Visuals 生成
  fonts.rs                 # 系统字体 + 内嵌 ShareTech（数字）
  i18n.rs                  # 中英文案表 + 覆盖率测试
  notify/                  # 通知策略与投递（Linux D-Bus / Windows 托盘气泡）
  tray/                    # 托盘：Linux ksni(SNI) / Windows tray-icon，菜单与命令队列
  instance.rs              # 单实例：Linux D-Bus 名 / Windows 命名互斥体+事件
  views/                   # 页面：status（余额/趋势/连接）、subscriptions、settings
rust-linux/                # 入口 12 行：调用 dsmon_ui::run()，产物名 dsmon
rust-windows/              # 入口 13 行 + build.rs/app.manifest（exe 图标与 DPI 声明）
packaging/                 # .desktop 与 deb/rpm 打包脚本
```

### 线程与事件模型

- **轮询线程**（`monitor.rs`）：按 `interval_minutes` 取数，写历史，裁剪过期数据，发布
  `Snapshot`；界面只读快照，从不阻塞在网络或数据库上。界面通过 channel 发送
  `Refresh` / `RefreshSubscriptions` / `Stop`。
- **界面回调**：eframe 的 `App::logic` **总会**被调用，`App::ui` 只在有窗口绘制时调用。
  因此托盘驱动、通知判定、关闭请求的应答、心跳重绘都在 `logic` 里——放进 `ui` 会在窗口
  最小化/被遮挡时失灵（曾因此表现为「关闭按钮时灵时不灵」）。
- **命令队列**：托盘菜单、单实例的另一进程、按钮都往同一个
  `Arc<Mutex<Vec<tray::Command>>>` 里写，`logic` 每帧取走。

### 数据与存储

- 配置：`~/.config/deepseek-balance-monitor/config.json`（Windows：`%APPDATA%\DeepSeek Balance Monitor\`）
- 状态目录（Linux 走 XDG state）：`~/.local/state/deepseek-balance-monitor/`，含
  - `dsmon.db`：**本版自己的库**，表 `balance_history`（带 `platform` 列）、
    `subscription_history`、`secure_settings`
  - `.secure_settings.key`：32 字节密钥，权限 0600
  - `app.log`：按天裁剪
- **1.x 的 `balance_history.db` 只读**（设置页「从 1.x 数据库导入」），本版绝不写入；
  两个版本可同时运行。`storage::import_from_legacy` 是唯一入口。
- 历史去重窗口 120 秒，时间戳格式 `%Y-%m-%d %H:%M:%S`（与 1.x 一致）。
- `DELETE` 不会缩小 SQLite 文件：设置页的「清除 N 天前的数据」才走
  `PRAGMA wal_checkpoint(TRUNCATE); VACUUM;` 真正回收空间。

### 密钥

- AES-256-GCM（ring，静态链入），密钥由系统熵源生成、存状态目录、权限 0600；
  密文格式 `DSBM1` + 12 字节 nonce + 密文 + 16 字节标签，AAD 绑定上下文。不依赖
  DPAPI / libsecret / Keychain。
- **API Key 永不写入 `config.json`**；按平台 key 存于 `secure_settings` 表
  （`deepseek`、`opencode_go`、`glm_coding_cn` …），一个平台一条。

### 平台集成矩阵

| 能力 | Linux | Windows |
|---|---|---|
| 托盘 | ksni（StatusNotifierItem，纯 D-Bus，无 GTK） | tray-icon（Win32 通知区，muda 菜单） |
| 通知 | `org.freedesktop.Notifications`（zbus 直连，无守护则静默） | 托盘气泡 `Shell_NotifyIconW` + NIF_INFO |
| 单实例 | 会话总线名 `com.github.wenyinos.deepseek-balance-monitor` + `Show` 方法 | 命名互斥体判定 + 命名事件传递唤出 |
| 自启 | `~/.config/autostart/deepseek-balance-monitor.desktop` | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| 窗口图标 | `_NET_WM_ICON`，128px | `ViewportCommand::Icon` + exe 资源（build.rs 嵌 `assets/app.ico`） |

**Linux 默认走 XWayland**（`app.rs::prefer_x11`）：Wayland 下窗口无法隐藏、也无法唤回
（winit 的 Wayland 后端 `set_visible` 是空实现、拒绝取消最小化、`focus_window` 为空），
而本应用的核心就是关闭进托盘。会话没有 X 显示时保持原生 Wayland；
`DSMON_NATIVE_WAYLAND=1` 可显式要求原生。打包时 `xwayland` 是硬依赖。

### 平台目录（catalog）

一个平台一条 `PlatformMeta`：`key`（也是密钥名与历史里的 provider）、`display_name`、
`mode`（`Payg` 余额 / `Package` 额度窗口）、`windows`（package 才有，如
`["5h","weekly","monthly"]`）、`console_url`、`implemented`。界面据此渲染：侧边栏列出
已配置的 payg 平台，订阅页按 package 平台逐张出卡片，设置页按 mode 分组列密钥输入框。
**加平台的成本 = 一条条目 + 一个客户端 + `monitor::fetch_package` 一行**。

额度统一为 `QuotaWindow`（已用份额、剩余份额、重置秒数，以及以金额计量时的 `used`/`cap`），
一个平台的读数是「窗口名 → 窗口」的 `PackageQuota`。

### i18n 与主题

- 全部文案在 `dsmon-ui/src/i18n.rs` 的 match 表里；`i18n::tests::every_key_the_interface_uses_is_answered`
  会遍历界面用到的键并要求中英都有值——新增文案必须同时加两侧。
- 6 套图标配色（default/contrast/bright/dark_mode/mono/custom，四色自定义）与界面主题
  相互独立；界面明暗切换只有一个入口：窗口左下角按钮。对比度有测试兜底（WCAG 相对亮度）。

## 发布与打包

- tag `v*` → 两个 workflow 都发布（`linux.yml` 出 .deb/.rpm，`windows.yml` 出签名 exe）；
  其余 push/PR 只做检查。
- Linux 在 `debian:12` 容器构建（glibc 2.36 基线，对应 Debian 12 / Ubuntu 24.04 /
  Fedora 38+），产物 `dsmon`；包依赖声明 `xwayland` 与 CJK 字体
  （`fonts-noto-cjk` / `google-noto-sans-cjk-fonts`）。
- Windows 走 SignPath 签名（见 `CODE_SIGNING.md`），只出 x86_64。

## 注意事项

- 改动 API 客户端时同步看 `PLATFORM_PORTING.md`：MiniMax 的剩余百分比、GLM 的窗口按位置
  识别、两者共用的秒/毫秒时间戳判断都最容易出错，且都有单测。
- egui 的控件 id 由标签推导：同一文案出现两次要加 `id_salt`。
- 托盘图标是**代码绘制**的（余额数字 + 状态色），不是图片；`assets/app.ico` 只用于窗口、
  任务栏与 exe。
- 数据库/密钥/认证相关改动容易造成用户数据损失：动 `storage.rs` 前先想清楚迁移与只读边界。
