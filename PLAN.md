# DeepSeek Balance Monitor 2.0 实施方案

纯 Rust 跨平台桌面应用重写方案。本文件是实施依据，所有决策已定稿。

## 1. 目标与范围

**产品形态**：Windows / Linux 双平台桌面应用，**同一份界面代码**，托盘常驻 + 主窗口 + 悬浮小工具。

**平台基线**：Windows 10 build 19041 (20H1) 及以上；Linux kernel 6.1 LTS 对应发行版及以上（Debian 12 / Ubuntu 24.04 / Fedora 38+）。

**不做**：KDE plasmoid、CLI 模式、Rainmeter、macOS、任何 WebView（Tauri）。

**保留的后端能力**：DeepSeek 余额、OpenCode Go 三窗口额度、Command Code 额度与月度档位、服务状态抓取、历史记录与忙时速率算法、CSV 导出、demo 演示模式。

**版本**：2.0.0。

**贯穿原则**：零残留、零兜底——旧实现全删，不写并行实现、不做兼容分支；平台差异只表达当前真实需要的差异。

## 2. 技术选型

| 层 | 选型 | 版本 | 说明 |
|---|---|---|---|
| GUI | eframe / egui | 0.36.2 | 纯 Rust 自绘，wgpu 后端；本机编译通过 24s |
| 图表 | egui_plot | 0.37.0 | 折线图 |
| 托盘 Windows | tray-icon | 0.25 | 仅 Windows target 引入 |
| 托盘 Linux | ksni | 0.3.6 | 纯 D-Bus（StatusNotifierItem），零系统依赖 |
| 通知 Linux | notify-rust | 最新 | D-Bus 桌面通知（ksni 不含此能力） |
| 加密 | ring | 0.17 | AES-256-GCM，静态链接进二进制 |
| Windows 系统调用 | windows | 最新 | 替代手写 FFI |
| 图标栅格化 | image + ab_glyph | 0.25 / 0.1 | 内嵌 ShareTech 数字字体 |
| 存储 / HTTP | rusqlite(bundled) / reqwest(rustls) | 沿用 | 数据格式不变 |
| 工具链 | Rust stable | 1.95.0 | 根 `rust-toolchain.toml` 统一 |

## 3. 视觉设计系统（COSMIC 风格）

### 3.1 配色

**深色模式**（青绿主色）

| 语义 | 值 | 用途 |
|---|---|---|
| `bg_app` | `#2b2b2b` | 窗口底色 |
| `bg_sidebar` | `#303030` | 侧边栏 |
| `bg_panel` | `#363636` | 卡片 |
| `bg_input` | `#3f3f3f` | 输入框 |
| `bg_hover` | `#404040` | 悬停行 |
| `border` | `#454545` | 分隔线 |
| `accent` | `#5ee0c8` | 主色 |
| `on_accent` | `#1a1a1a` | 主色上的文字 |
| `text_primary` | `#e6e6e6` | 主文字 |
| `text_secondary` | `#a0a0a0` | 次要文字 |
| `positive` | `#7ee0a8` | 正常态 |
| `warning` | `#e8c07a` | 额度 >60% |
| `destructive` | `#f0a0a8` | 异常、额度 >80% |

**浅色模式**（深蓝主色）

| 语义 | 值 |
|---|---|
| `bg_app` | `#f0f0f0` |
| `bg_sidebar` | `#fafafa` |
| `bg_panel` | `#ffffff` |
| `bg_input` | `#f5f5f5` |
| `bg_hover` | `#e8e8e8` |
| `border` | `#e0e0e0` |
| `accent` | `#1a6a94` |
| `on_accent` | `#ffffff` |
| `text_primary` | `#1a1a1a` |
| `text_secondary` | `#5a5a5a` |
| `positive` | `#2e7d32` |
| `warning` | `#8a6100` |
| `destructive` | `#b3261e` |

浅色模式主色换深蓝是刻意的：青绿在白底上对比度不足。

### 3.2 组件规格

扁平化设计，无聚焦光晕、无阴影。

| 组件 | 规格 |
|---|---|
| 主按钮 | accent 填充 + `on_accent` 文字，圆角 8，高 36 |
| 次按钮 | `bg_input` 填充 + `text_primary`，圆角 8 |
| Positive / Destructive | 同主按钮，填充改为对应语义色 |
| Text 按钮 | 无背景，仅文字 |
| 禁用态 | 统一降透明度至 40% |
| 输入框 | `bg_input` 填充、圆角 8，聚焦时边框转 accent |
| 侧边栏项 | 高 36、圆角 18（胶囊）；选中项 accent 填充 + `on_accent`，未选中仅图标 + `text_primary` |
| 设置行 | 卡片内左标签右控件，行间 1px `border` 分隔线 |
| 开关 | 42×24 胶囊，选中 accent |
| 滑块 | 2px 细轨 + 13px 圆点手柄 |
| 进度条 | 3px 细线，填充按阈值切 accent → warning → destructive |
| 复选框 | 18×18、圆角 4，选中 accent 填充 |
| 单选 | 18px 圆形，选中为 accent 实心圆点 |
| 卡片 | 圆角 12、内边距 16 |
| 图表 | 曲线用 accent，网格用低对比 border 色；跟随明暗主题 |

### 3.3 字体

| 用途 | 来源 |
|---|---|
| 界面正文与中文 | **系统原生字体**（Windows：微软雅黑 + Segoe UI；Linux：fontconfig 解析 Noto CJK / 思源黑体） |
| 余额数字 | 内嵌 `assets/font/ShareTech-Regular.ttf` |

CJK 字体缺失风险由 rpm/deb 依赖声明消除（`fonts-noto-cjk` / `google-noto-sans-cjk-fonts`）。内嵌体积仅几十 KB。

### 3.4 形状与间距

基础单位 8px：组件内边距 10×6、区块间距 16、卡片间距 12。圆角：按钮/输入 8、卡片 12、侧边栏胶囊 18（半高）。

## 4. 主题系统

- 两套 `egui::Visuals` 由 3.1 语义色生成，所有视图只引用语义名，不写死颜色
- 三档模式 `system` / `light` / `dark`，存配置字段 `ui_theme`（默认 `system`）
- 跟随系统：Windows 读注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`；Linux 走 D-Bus `org.freedesktop.portal.Settings` 的 `color-scheme`（1=dark、2=light），复用 ksni 已引入的 zbus
- 切换入口：设置页三选一 + 托盘菜单快捷切换，即时生效
- **图标配色独立于界面主题**：保留现有 6 套预设（default/contrast/bright/dark_mode/mono/custom）与四色自定义

## 5. 架构

```
DeepSeekBalanceMonitor/
├── Cargo.toml / rust-toolchain.toml      # workspace，stable
├── dsmon-core/                           # 平台无关，无 GUI 依赖
│   ├── model.rs      # Balance / HistoryRecord / ConsumptionRate / 各平台额度
│   ├── config.rs     # AppConfig 与新字段
│   ├── paths.rs      # Win: %APPDATA% / Linux: XDG
│   ├── crypto.rs     # AES-256-GCM
│   ├── storage.rs    # SQLite 建表、历史 CRUD、保留策略
│   ├── history.rs    # 忙时速率、摘要、CSV 导出
│   ├── platforms/    # deepseek / opencode_go / command_code / status
│   ├── icon.rs       # 托盘图标位图渲染
│   └── demo.rs       # 演示模式
├── dsmon-ui/                             # 唯一一份界面
│   ├── theme.rs      # 双主题 + 语义色 + Visuals 生成
│   ├── fonts.rs      # 系统字体加载 + 内嵌数字字体
│   ├── i18n.rs       # 中英文案
│   ├── app.rs / state.rs
│   ├── views/{status,history,settings}.rs
│   ├── widget.rs     # 悬浮小工具
│   ├── notify.rs     # 系统通知抽象
│   └── tray/{mod,windows,linux}.rs
├── rust-linux/                           # 薄入口 ~30 行
└── rust-windows/                         # 薄入口 ~30 行 + app.ico / app.manifest / build.rs
```

两个 bin 都只调用 `dsmon_ui::run()`，界面代码在仓库中只存在一份。

**主窗口布局**：左侧边栏（宽 190，顶部应用名 + 状态/历史/设置三个胶囊导航项）；标题栏走系统原生，不做自绘。

## 6. 关键设计

**加密**：AES-256-GCM，用 `ring` 静态链接进二进制；32 字节密钥由系统熵源生成、存数据目录、权限 0600；密文格式 `DSBM1` + 12 字节 nonce + 密文 + 16 字节标签，AAD 绑定上下文。不依赖 DPAPI / libsecret / Keychain。两平台同一份代码、同一密文格式。Windows 老用户需重输一次密钥。

**数据契约**：SQLite 表结构与时间戳格式（`%Y-%m-%d %H:%M:%S`）、120 秒去重窗口沿用现有定义，老用户历史继续可读。

**线程模型**：core 后台线程按 `interval_minutes` 取数、写历史、裁剪过期数据；UI 读共享快照；托盘与 IPC 命令经 channel 进入 UI 主循环。

**平台差异收敛**：托盘（tray-icon / ksni）、通知（托盘气泡 / notify-rust）、自启（启动目录 lnk / XDG autostart）、单实例（命名管道 / D-Bus 服务名）、无控制台启动（`windows_subsystem`）——集中在少数文件，不污染 UI 层。

**图标渲染**：`image` + `ab_glyph` + 内嵌 ShareTech 绘制圆角底板与余额数字，输出 RGBA 位图供两个托盘库使用；沿用现有数字规则（超两位显示 `OK`）与 6 套配色预设。

## 7. 交互行为

| 场景 | 行为 |
|---|---|
| 关闭主窗口 | 最小化到托盘，首次关闭时提示一次"仍在后台运行" |
| 启动（手动） | 显示主窗口 |
| 启动（开机自启） | 静默驻留托盘，不弹窗 |
| 启动（首次运行/未配密钥） | 显示主窗口并引导配置 |
| 重复启动 | 第二个实例唤出已有窗口后退出 |
| 告警触发 | 系统通知：Windows 托盘气泡、Linux D-Bus 通知；由 `alert_mode`（never/always/once）与 `api_alert_enabled` 控制 |
| 托盘左键 | 切换悬浮小工具显隐 |
| 托盘右键 | 打开主窗口 / 小工具开关 / 立即刷新 / 明暗切换 / 设置 / 退出 |

## 8. 界面设计

**状态页**：余额卡片（ShareTech 大字金额、币种、更新时间、刷新按钮）；状态行卡片（服务健康点、忙时速率、预计可用时长）；订阅卡片组（OpenCode Go 三窗口与 Command Code 月度，各一条细线进度条 + 百分比）；底部上次检查时间。

**历史页**：筛选卡片（24h / 7d / 30d + 币种下拉，一次一条曲线）；图表卡片（egui_plot 折线图）；摘要卡片（总余额、期间消耗、趋势）+ 导出 CSV。

**设置页**：五张卡片——账户（三平台密钥，含"测试连接"按钮）、通用（间隔、界面语言、界面主题、图标配色、代理）、告警（阈值、模式）、数据（保留天数、导出路径）、关于（版本号 + 打开 Releases 页链接）；底部保存 / 取消。

**悬浮小工具**：无边框半透明卡片，始终置顶（可关闭）、尺寸两档（紧凑约 160×80 仅余额 / 标准约 220×110 含更新时间）、透明度 50%–100% 可调、带迷你趋势线（近 24 小时走向）；显示 DeepSeek 主币种余额 + 状态色点；可拖动、双击唤出主窗口、右键菜单；默认右上角、默认不显示。

**通知**：低余额、服务状态变化、密钥缺失、数据库重建四类；Linux 上若无通知守护则静默。

## 9. 配置模型

**新增**：`ui_theme`、`widget_enabled`、`widget_size`、`widget_opacity`、`widget_always_on_top`、`widget_show_trend`、`widget_pos`、主窗口尺寸与位置、首次关闭提示已显示标记。

**移除**：`language`（CLI 输出语言）、`api_key`（早已迁至加密存储）、`extra`（未知字段兜底）、以及旧字段迁移代码。

**保留**：`interval_minutes`、`threshold_yuan`、`ui_language`、`auto_start`、`alert_mode`、`api_alert_enabled`、`retention_days`、`export_path`、`http_proxy`、`proxy_enabled`、`theme`、`icon_colors`、`icon_stroke`。

代价：从很老版本升级时，告警模式等回到默认值，需重设一次。

## 10. 删除清单与迁移映射

**彻底删除**：

- `rust-linux/src/main.rs` 全部（CLI 参数层、`print_*`、`set-key`/`set` 子命令、`WidgetStatus`/`ConfigJson`/`HistoryReport`）
- `rust-windows/src/main.rs` 全部（nwg 的 AppUi 与 SettingsWindow、Rainmeter HTTP 服务、手写 Win32/COM/DPAPI FFI、托盘位图绘制、ASCII 报告）
- 依赖：`native-windows-gui`、`rusttype`、`imageproc`、Windows 侧死依赖 `ring`
- 两个子目录的 `rust-toolchain.toml` 与两套 `Cargo.lock`

**搬入 core**：`AppConfig` 主体、全部领域结构（补齐 `Serialize`）、三平台 API 客户端与解析、`epoch_to_reset_seconds`、月度档位表、`urlencode`、`sanitize_*`、服务状态抓取与归一化、`save_balance_history` / `history_records` / `summarize_history`、忙时速率算法、`history_csv`、`prune_balance_history`、`open_history_db` 迁移逻辑、日志裁剪、`demo`。

**统一分歧**（两版实现不一致处）：

| 项 | 采用 |
|---|---|
| `recent_balance_history` | 取最近 N 条（Linux 语义） |
| `fetch_balance` | ASCII 清洗 + `Invalid API Key` 文案（Windows 版） |
| `is_low_balance` | 无状态签名 |
| `log_line` | 注入路径并返回 Result |

**文案**：现有 150 条双语表搬入 `dsmon-ui/i18n.rs`，键名不变。

## 11. 测试策略

`dsmon-core` 全面单测：API 响应解析（DeepSeek / OpenCode Go / Command Code）、月度档位映射、忙时速率算法、加密回环（加解密一致 + 篡改检测）、SQLite 读写与去重、CSV 导出、配置序列化。现有 6 个测试重写迁入。UI 层不做自动化测试，人工验证。

## 12. 实施阶段

| 阶段 | 内容 | 验证点 |
|---|---|---|
| 0 | workspace + 工具链 + 主题系统 + 字体加载 + 最小窗口 + Linux 托盘 | 本机 `cargo run` 出窗口、双主题可切、中文正常、托盘可点 |
| 1 | `dsmon-core` 全量 | `cargo test -p dsmon-core` 通过 |
| 2 | 三页界面 + 小工具 | 手动跑通刷新、保存、图表 |
| 3 | 托盘交互 + 通知 + 平台集成（自启、单实例、无控制台） | Linux 实测；Windows 推 CI |
| 4 | 两个 bin 塌缩为薄入口，旧 `main.rs` 删除 | 无旧实现残留 |
| 5 | CI 重写（含 rpm/deb 骨架）、文档重写 | CI 绿灯 |

## 13. CI 与打包

- **rust-linux.yml**：去掉 Rust 1.77.2 与 glibc 2.28 检查；构建环境改 debian:12 容器（glibc 2.36 基线，匹配目标发行版）；产物为 GUI 二进制，打包走 rpm/deb，声明 CJK 字体依赖
- **rust-windows.yml**：工具链更新至 stable，SignPath 签名流程保留
- 构建交给 CI，本机只做 Linux 侧开发与验证

## 14. 风险

| 风险 | 应对 |
|---|---|
| 8671 行旧代码收敛为两个薄入口，重写量大 | 后端逻辑按函数清单逐项搬运，UI 全新 |
| Windows 老用户密钥失效 | 首次启动提示重新输入 |
| Windows 编译本机无法验证 | 阶段 0 结束即推 CI 打通 |
| 无 GPU 环境 wgpu 起不来 | 阶段 0 实测，必要时切 glow 后端 |
| egui 无原生发光效果 | 已改为纯扁平，不再依赖 |
| 两平台字形差异 | 已接受；包管理器声明字体保证不缺字 |
| GNOME 默认桌面不显示托盘 | 文档说明需 AppIndicator 扩展 |
| Linux 无通知守护 | 静默降级 |

## 15. 已按推荐设定的默认细节

主窗口默认 420×560、尺寸位置记忆；快捷键 Ctrl+S 保存、Esc 关闭设置、Ctrl+Q 退出；设置页保存密钥提供"测试连接"按钮但不强制验证；日志 `app.log` 与按天裁剪沿用；小工具显示 DeepSeek 主币种余额。
