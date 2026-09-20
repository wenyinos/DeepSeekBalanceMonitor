# AGENTS.md

跨平台 DeepSeek 余额监控（2.0，纯 Rust）。**权威项目细节见 `CLAUDE.md`**（架构、存储格式、
平台集成矩阵、打包），本文件只列 agent 容易踩坑的高信号事实。

设计文档都在 `docs/`（索引见 `docs/README.md`）：1.x 与 2.0 的差异见 `docs/GAPS.md`，
Rust 与 Python 版应共同暴露的接口见 `docs/INTERFACES.md`。

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
- **状态页只接受「第一个密钥交换组是后量子混合组」的握手**：`status.deepseek.com` 前面的边缘节点
  会重置第一个组不是 `X25519MLKEM768` 的握手（只报 `X25519` 被重置，把混合组排在后面也被重置，
  服务端一言不发），而 reqwest 0.11 内置的 rustls 0.21 根本没有后量子组——服务状态因此连续四天
  显示「未知」（2026-09-20 定位）。现在 `platforms::http_client` 自己装 aws-lc-rs provider
  （rustls 的 `prefer-post-quantum` 把混合组排第一），reqwest 用 no-provider 特性。**这两处别改
  回去**，也别换成系统 TLS（要求用内置 TLS）。同类现象排查手法：curl/wget/go 能通而程序
  不通，就用 `openssl s_client -groups <组名>` 对照，能立刻看出是握手被挑还是网络不通。
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
- **测试不得写真实数据目录**：`crypto` 的用例加密/解密时会经 `load_or_create_key()` 落到
  `paths::secret_key_file()`，密钥文件不存在就**创建**一份（连同目录）。现在由
  `test_support::state_in_a_scratch_directory()`（`#[cfg(test)]`）一次性把状态/配置目录指向
  临时目录，`crypto` 与 `paths` 的用例都先调用它——**今后任何碰路径的新测试也必须先调用**。
  没有这层保护就会写进用户真实目录，还会挡住首启从旧目录的搬移（搬移对"目标已存在"是跳过），
  于是数据库搬来了、密钥没搬来、库里所有密钥都解不开（2026-09-15 真机踩过）。真出事了不是没救：
  真密钥仍在旧目录 `~/.local/state/deepseek-balance-monitor/.secure_settings.key`（Windows
  `%APPDATA%\DeepSeek Balance Monitor\`），拷回新目录即可。
- **egui 里不要让热区重叠**（2026-09-15 踩过两次）：① 小工具标题条整行曾是 `Sense::drag()`，
  六个按钮叠在上面，结果**按钮集体失效**——按下只要被判成拖动，`ViewportCommand::StartDrag` 就把
  指针交给窗口管理器，按钮再也收不到释放；拖动因此改到左/右/下三条 8pt 边带，标题条改
  `Sense::hover()`。② 三条移动带把底边占满后，改高度的角热区落不到手，所以角要**后注册**
  （egui 里后注册的更靠上、命中优先）。
- **无边框窗口改尺寸只能自己发请求**：`with_resizable(true)` 不等于用户能改，X11 下没有可拖的
  边框，必须 `ViewportCommand::BeginResize(方向)`。小工具的两个角用 `South`（只改高度）而不是
  `SouthWest`/`SouthEast`——宽度是锁死的（`lock_width` 每帧把宽度拉回预设，它发的
  `InnerSize` 里带着高度），对角方向会让窗口管理器同时改宽高，用户拖出来的高度又被写回去。
- **平台代码有一半本地编不到**：小工具的 Windows 分支在 `#[cfg(windows)]` 里，本地只跑
  `cargo fmt`——rustfmt 会解析 cfg 掉的模块，所以**语法**错误它能报，**类型/名称**错误报不出来
  （`use std::sync::Arc` 少了这一句就是人工审读才发现的）。动了 Windows 分支就 push 一次让
  `windows.yml` 编过再算数。
- **Windows 的托盘注册要自己问**：图标库对 `Shell_NotifyIconW(NIM_ADD)` 失败不报错，所以
  小工具的 `Tray::report()`（每帧、只在答案变化时写日志）用 `TrayIcon::rect()` 反查系统是否真的
  持有图标——与主程序同一条教训，只是小工具不需要据此改变行为。
- **托盘不在就不许藏窗口**：`Tray::is_registered()` 是唯一依据——登录启动若外壳还没接收
  图标，窗口会留在屏幕上（等 15 秒后放弃隐藏）。「程序在跑、屏幕上一个东西都没有」就是这么来的。
- **API Key 永不写入 `config.json`**：按平台 key 加密存于 SQLite `secure_settings`。
  1.x 的 `balance_history.db` 只读，本版用 `dsmon.db`，两者互不影响。
- **数据库删除不等于缩小文件**：只有 `wal_checkpoint + VACUUM`（设置页的手动清理）才回收空间。
- **迁移只许一个连接做**：`storage::open_db()` 的建表与补列是「看了再改」，两个连接同时做会撞
  两次——同一列加两遍（第二个被告知已存在，错误文本是 `duplicate column name`），以及写锁互抢
  （`database is locked`）。**升级后的第一次启动正好是两边一起到**：轮询线程先起，界面的密钥
  查询紧跟其后。2.1.2 上界面那次抢输了，于是一个密钥都没读到、弹「尚未配置」并跳到设置页——
  而密钥一直在库里、能被解开（2026-09-16 真机踩过，重启即好）。现在 `open_db()` 用一把进程内锁
  把整段串起来，补列另外容忍 `duplicate column name`（`docs/INTERFACES.md` 允许另一个实现共用
  同一个库，那种情况下这把锁护不住对面）。**补列必须在建表之后、建索引之前**：`subscription_history`
  的 `window` 列由迁移补上，而 `idx_subscription_history_window` 引用它——补早了表还不存在
  （`no such table`，全新安装直接开不了库），补晚了索引建不起来。
- **文案**：全部在 `dsmon-ui/src/i18n.rs`，中英都要加；`every_key_the_interface_uses_is_answered`
  测试会检查界面用到的每个键。
- 清理测试实例时按路径锚定匹配（`pkill -f '^\./target/debug/dsmon2'`），不要用 `pkill -x`：
  本机可能装着 1.x 的 `/usr/local/bin/dsmon`（systemd 用户服务），按名字匹配会误杀。2.0 的可执行
  文件已改名为 `dsmon2` 以避免同名，但 `-x` 按名字匹配的风险依然存在。

## 发布触发

- tag `v*` → Linux（.deb/.rpm）与 Windows（签名 exe）两个 workflow 都发布；push/PR 只检查。
- Linux 在 `debian:12` 容器构建（glibc 2.36 基线），包依赖含 `xwayland` 与 CJK 字体。
- 签名细节见 `docs/CODE_SIGNING.md`。
