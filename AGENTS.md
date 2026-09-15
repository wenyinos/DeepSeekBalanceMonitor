# AGENTS.md

跨平台 DeepSeek 余额监控（2.0，纯 Rust）。**权威项目细节见 `CLAUDE.md`**（架构、存储格式、
平台集成矩阵、打包），本文件只列 agent 容易踩坑的高信号事实。

1.x 文档与 2.0 的差异、以及尚未移植的功能，见 `GAPS.md`。

## 命令与工具链

- 工具链为 **stable**（根 `rust-toolchain.toml`）；1.x 的 `cargo +1.77.2` 已作废，勿再用。
- 测试：`cargo test --workspace --locked`；格式：`cargo fmt --all --check`。
- 可执行文件叫 `dsmon2`（Windows 上是 `dsmon2.exe`），入口在 `dsmon-ui/src/bin/dsmon2.rs`；
  构建用 `cargo build --release -p dsmon-ui --bin dsmon2`。**刻意不与 1.x 的 `dsmon` /
  `deepseek-balance-monitor.exe` 重名**，两个版本要能并存。
- 开发时启动界面：`cargo run -p dsmon-ui --example preview`（不走平台入口）。
- 两个入口都是十几行，只调用 `dsmon_ui::run()`；界面代码在仓库里只有一份。

## 关键陷阱

- **`App::logic` 与 `App::ui` 的区别**：eframe 只在有窗口绘制时调用 `ui`。托盘驱动、通知
  判定、关闭请求的应答必须放在 `logic`，否则窗口最小化/被遮挡时这些功能会静默失效。
- **egui 关闭请求**：窗口关闭要发 `ViewportCommand::CancelClose` 才拦得住；eframe 还会在
  第一帧后强制 `set_visible(true)`，所以「启动即隐藏」要等首帧之后再做。
- **Linux 默认走 XWayland**（`app.rs::prefer_x11`）：Wayland 下窗口既不能隐藏也不能唤回。
  测试本机行为时注意 `DSMON_NATIVE_WAYLAND=1` 会切到原生 Wayland，行为不同。
- **X11 图标尺寸上限**：`_NET_WM_ICON` 单次属性请求最多 65535 个字，256×256 图标超两个字会
  被静默丢弃（窗口变成没有图标）。窗口图标固定用 128px。
- **依赖 feature 会互相打架**：`ksni` 的默认 feature 会打开 `zbus/tokio`，导致 zbus 选择
  Tokio 执行器而在本进程里 panic（没有 Tokio 运行时）。workspace 里已关掉 ksni 默认 feature
  并显式选 `async-io`，勿改回去。
- **托盘图标是代码绘制的**（余额数字 + 状态色块），不是图片文件；`assets/app.ico` 只用于
  窗口/任务栏与 Windows exe。
- **Windows 托盘两处易错**：① 气泡要靠编号指认图标，而 `tray-icon` 的编号是「从 1 起、
  每个图标消耗两个号」（字符串 id 一个、系统看到的一个），本程序唯一的图标是 **2 号**；
  编号被拒时 `notify/windows.rs` 用 `Shell_NotifyIconGetRect` 反查真实编号再试一次。
  ② 图标库对 `Shell_NotifyIconW(NIM_ADD)` 失败**不报错**，所以隐藏窗口前必须问
  `Tray::is_registered()`，否则会出现「程序在跑、屏幕上一个东西都没有」。
- **数据目录与 1.x 彻底分开**：本版用 `dsmon2`（Windows `%APPDATA%\dsmon2`，Linux
  `~/.config/dsmon2` + `~/.local/state/dsmon2`）；1.x 的目录只被只读导入。两版曾经共用
  目录，本版把 1.x 的 `config.json`/`app.log` 覆盖过，所以首启会用 `adopt::earlier_files`
  把本版遗留的文件搬出来（只搬本版自己的：`dsmon.db`、`.secure_settings.key`、
  `.dsmon.db.initialized`，以及靠独有字段辨认出的本版 `config.json`）。
- **托盘不在就不许藏窗口**：`Tray::is_registered()` 是唯一依据——登录启动若外壳还没接收
  图标，窗口会留在屏幕上（等 15 秒后放弃隐藏）。「程序在跑、屏幕上一个东西都没有」就是这么来的。
- **API Key 永不写入 `config.json`**：按平台 key 加密存于 SQLite `secure_settings`。
  1.x 的 `balance_history.db` 只读，本版用 `dsmon.db`，两者互不影响。
- **数据库删除不等于缩小文件**：只有 `wal_checkpoint + VACUUM`（设置页的手动清理）才回收空间。
- **文案**：全部在 `dsmon-ui/src/i18n.rs`，中英都要加；`every_key_the_interface_uses_is_answered`
  测试会检查界面用到的每个键。
- 清理测试实例时按路径锚定匹配（`pkill -f '^\./target/debug/dsmon2'`），不要用 `pkill -x`：
  本机可能装着 1.x 的 `/usr/local/bin/dsmon`（systemd 用户服务），按名字匹配会误杀。2.0 的可执行
  文件已改名为 `dsmon2` 以避免同名，但 `-x` 按名字匹配的风险依然存在。

## 发布触发

- tag `v*` → Linux（.deb/.rpm）与 Windows（签名 exe）两个 workflow 都发布；push/PR 只检查。
- Linux 在 `debian:12` 容器构建（glibc 2.36 基线），包依赖含 `xwayland` 与 CJK 字体。
- 签名细节见 `CODE_SIGNING.md`。
