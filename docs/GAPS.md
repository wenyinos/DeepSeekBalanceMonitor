# 1.x → 2.0 差异与后续改进

2.0 是纯 Rust 重写（重写计划 `PLAN.md` 在 2.0 落地后删除，只留在 git 历史里），1.x 的文档里有不少内容描述的是已经删除的 Python/tkinter
实现。`CONTRIBUTING.md` 保持原样未动，本文件把差异与功能缺口单独列出，作为后续改进清单。

- **本文只描述差异与待办，不改写 1.x 文档**；`CONTRIBUTING.md` 里仍然适用的知识见第二节。
- 建议处置列的含义：**补** = 建议在后续版本实现；**弃** = 建议明确不做；**待定** = 需要决定。
- 代码位置以当前仓库为准（`dsmon-core/`、`dsmon-ui/`）。

---

## 一、整段失效（描述的代码已不存在）

| 1.x 文档位置 | 原文描述 | 2.0 现状 |
|---|---|---|
| 抬头（3–5 行） | 「描述 v2.0.2 的 Python-Windows 运行时架构」 | 纯 Rust；版本号 2.1.2（1.x 已到 1.5.0） |
| 抬头（5 行） | 「Python 与 Rust 双实现，改 API 客户端/忙时算法/告警逻辑必须同步检查两端」 | **该约定作废**：Python 实现与 `src/` 已整体删除，只剩一套 Rust |
| 项目状态（7–9 行） | 15 平台、管理页设为首选、并行/单打双查询、双实现 | 14 个平台条目、无「首选」概念、无查询模式开关 |
| 架构总览（11–47 行） | `src/**` 全树、`integrations/rainmeter_server.py`、`mac/`、`webview/`、依赖方向与循环依赖消解 | 目录已删。现在依赖是单向的：平台入口 → `dsmon-ui` → `dsmon-core`，结构上不可能成环 |
| 主窗口结构（49–55 行） | tkinter 懒构建 Tab、DPI 高度公式、Canvas 固定物理像素 | egui 即时模式；侧边栏页面而非 Tab |
| 管理 Tab（95–104 行） | 多 API 增删改查、⭐设为首选、ledger 联动 | 无多账户模型：一平台一 key，在设置页填写 |
| 看板（106–127 行） | HistoryFrame 三图表块、悬浮提示、API 选择器、GitHub 风格热力图 | 余额页为「余额卡 + 趋势摘要 + 连接状态 + 折线图」 |
| 设置页排版（153–163 行） | Tk 单行化、语言切换销毁重建窗口、未保存弹框 | 五张卡片 + 保存/取消；语言切换即时生效 |
| 关键文件清单（182–198 行） | 全部是 `src/**.py` 路径 | 文件均不存在 |
| 开发注意事项（203–215 行） | ttkbootstrap/PyInstaller、PowerShell BOM、Tk 陷阱集、`from X import log` 局部名陷阱 | Python/Tk 专有，不再适用 |

### 值得保留的一条教训（换了位置）

1.x：「懒构建 `_ensure()` 必须返回 `win`，否则首次打开全链路失效」。
2.0 的同类事故是：**关闭请求与托盘驱动必须放在 `App::logic`，放进 `App::ui` 会在窗口最小化或
被遮挡时静默失效**（eframe 只在有窗口绘制时调用 `ui`）。已写入 `AGENTS.md`。

---

## 二、仍然适用（1.x 的平台知识，2.0 已按同样结论实现）

改动解析逻辑时先看这些，它们没有因为重写而变化：

| 1.x 文档位置 | 内容 | 2.0 对应实现 |
|---|---|---|
| 68–70 行 | GLM 端点、Bearer 认证、401 回退裸 Key 一次、`TOKENS_LIMIT` 第 0/1 条→5h/weekly、`TIME_LIMIT`→monthly | `platforms/glm.rs`（有单测） |
| 71–72 行 | OpenRouter 仅 Management Key 可用，无 `/key` 降级 | `platforms/openrouter.rs` |
| 76–80 行 | Command Code 档位表（Go 10 / GOAT 70 / Pro 80 / Max 10× 150 / Max 20× 300 / Team Pro 40）与「由 5h/周 cap 反推月度 cap」 | `platforms/command_code.rs::monthly_cap` |
| 83 行 | 窗口数据形状、`resetAt` 秒/毫秒归一、`used/cap` 兼容数字或数字字符串 | `QuotaWindow`、`epoch_to_reset_seconds`、`deserialize_number` |
| 133–136 行 | payg 三字段映射；Kimi `available/voucher/cash`；StepFun `balance/total_cash_balance/total_voucher_balance` | `platforms/kimi.rs`、`platforms/stepfun.rs` |
| 142 行 | 状态写入只写本平台，无状态页的平台写 NULL，**禁止借用首选平台状态** | 同规则（非 DeepSeek 记 `unknown`） |
| 179 行 | rustls + webpki-roots 内嵌证书 | 同（`reqwest` 的 rustls 后端） |
| 202 行 | API Key 不进 `config.json` | 同（加密方式由 Fernet 换成内置 AES-256-GCM） |
| 207 行 | 所有 UI 文本集中在文案表 | 同（`i18n.rs` + 覆盖率测试） |

---

## 三、描述与新版行为不同（读 1.x 文档时要注意）

1. **版本号**：1.x 文档写到 v2.0.2（Python 线末版），本分支是 **2.1.2**（新增桌面小工具，
   详见 `WIDGET.md`）。
2. **平台清单**：15 个（含 `command_code_goat`）→ **14 个**，没有 goat 条目；以
   `catalog.rs` 的 `PLATFORMS` 为准。
3. **额度口径相反**：1.x 以 `percent_remaining` 为主、`usage_percent` 派生、剩余可 >100% 不
   clamp；2.0 以 **`usage_percent` 为主**、`percent_remaining` 派生、统一 clamp 到 `[0,100]`，
   金额计量的窗口（Command Code 月度）直接存 `used`/`cap`。
4. **注册表字段**：1.x 的 `default_billing_period`、`window_pools`、`has_status_page` 在 2.0
   都不存在——没有 per-API 计费周期、没有插值池、状态页只有 DeepSeek 且按 key 判定。
5. **加平台成本**：1.x「注册表加一行」→ 2.0 **三步**：catalog 条目 + 客户端 +
   `monitor::fetch_package` 一行；界面不用动。
6. **历史表结构**：1.x 是 `api_id` 列 + `balance_history`/`package_history`；2.0 是
   `platform` 列 + `balance_history`/`subscription_history`，订阅表存 `used/cap`。
7. **托盘菜单**：1.x「⚡余额速览 / 看板 / API选择 / 立即查询 / 控制台 / 设置」→ 2.0
   「查看余额 / 打开主窗口 / 立即查询 / 显示·隐藏桌面小工具 / 设置 / 退出」。
8. **构建与验证**：rockylinux:8 + Rust 1.77.2 + glibc 2.28 → **debian:12 + stable + glibc 2.36**，
   产物为 .deb/.rpm（依赖含 xwayland 与 CJK 字体），**主程序与小工具各一份**；Windows 侧同理，
   各一个 MSI。
9. **桌面小工具**：1.x 是 Rainmeter 皮肤（监听 `17654`，皮肤工程独立于程序）→ 2.0 是自带的第二个
   可执行文件 `dsmon2-widget`（监听 `18964`，只读、只在本机）。两者端口不同，可以与 1.x 的皮肤
   同时运行；接口契约见 `docs/INTERFACES.md`。

---

## 四、功能缺口（后续改进清单）

1.x 有、2.x 尚未实现的功能。每条给出 1.x 行为、当前现状、移植要点与建议。

**状态**（2026-09-15 更新，逐条对代码核实）：

| 条目 | 状态 |
|---|---|
| 4.1 OCGo 周/月剩余精化 | **已做**（`history::refined_percent` + 历史表新增 `window` 列） |
| 4.2 MiniMax TLS 重试 | **已做**（三次重试 + `Connection: close`；被拒的密钥不重试） |
| 4.3 MiniMax 服务状态页 | **不做**：MiniMax 在本项目是订阅，订阅卡没有状态行的位置 |
| 4.4 峰谷时提醒 | **已做**（`time::is_off_peak_at`，默认开，设置页可关） |
| 4.5 单日消耗过快提醒 | **已做**（托盘图标第五态 + 每天一次的提醒，线值默认 0 = 关） |
| 4.6 多 API 账户与「首选」 | 建议维持不做 |
| 4.7 热力图 + 时段分布图 | 热力图**已做**（2.1 的小工具，12 周、可按订阅切换）；时段分布图仍未做 |

顺带修掉一个既有缺陷：DeepSeek 的状态页原先读 `status.flashcat.cloud/deepseek`，那个页面里只有
FlashDuty 自己的 `Open API` 组件，于是状态恒为「正常」；改用厂商自己的 `status.deepseek.com`
（同一个 FlashDuty 系统，但组件写在 HTML 里）后能读到真实状态。

### 4.1 OCGo 周/月剩余精化 —— 建议：补（中）

- **1.x 行为**：API 的周/月剩余是整数百分比（1% 步长），1.x 用 5h 窗口的真实消耗把它插值成
  连续小数（如 70 → 70.43）。取整语义经实验确认为 **round**；池比 5h=$12 / 周=$30 / 月=$60；
  模型为「剩余 = 100 − (起步 usage+0.5 + Σ 每行区间真实 5h 消耗$ ÷ 每粗% 平均$)」，
  **仅按真实消耗推进、严格因果**（每点只用前驱），无 5h 消耗行保持平段。
- **2.0 现状**：`platforms/opencode_go.rs` 直接返回 API 原始整数百分比，订阅页照实显示。
- **移植要点**：需要 `window_pools` 一类配置（或写死 OCGo 的池比）、历史里的 5h 消耗序列
  （`subscription_history` 目前只记月度窗口）、以及一个纯函数 + 单测。
- **取舍**：属于显示精度改进，不影响告警正确性。

### 4.2 MiniMax TLS 重试 —— 建议：补（小）

- **1.x 行为**：MiniMax 偶发 `UNEXPECTED_EOF`，`fetch_minimax_quota` 内重试 3 次（间隔 1 秒）
  并显式发送 `Connection: close`。
- **2.0 现状**：未实现，一次失败即在卡片上显示错误（下一轮轮询会重试）。
- **移植要点**：在 `platforms/minimax.rs` 的请求处加重试循环；`http_client` 里加
  `Connection: close` 头。

### 4.3 MiniMax 服务状态页 —— 建议：待定（小）

- **1.x 行为**：抓 `status.minimax.io` 的 LLM 组件状态（HTML 解析）。
- **2.0 现状**：状态页只有 DeepSeek（`platforms/status.rs`），其它平台显示「连接状态」。
- **取舍**：接入后需要为「非 DeepSeek 状态页」设计展示位置（余额页第三张卡目前按 key 分叉）。

### 4.4 峰谷时提醒 —— 建议：待定（小）

- **1.x 行为**：GMT+8 周一至五 9–12 / 14–18 为峰时，其余为谷时；相位翻转时通知一次；
  仅在首选平台为 DeepSeek 时生效。
- **2.0 现状**：无。
- **取舍**：与「低额提醒」是并列的告警类型，要加就要进设置页的告警卡片与通知策略。

### 4.5 单日消耗过快提醒 —— 建议：待定（小）

- **1.x 行为**：当日忙时正增量达到设定线值时通知一次；托盘图标同步变橙。
- **2.0 现状**：无（图标只有 正常/低额/服务异常/无数据 四态）。
- **取舍**：需要新增配置项（线值）、一个「当日消耗」统计（`history.rs` 已有 `daily_usage`，
  可直接用）以及图标第五种状态色（会牵动图标配色预设表与对比度测试）。

### 4.6 多 API 账户与「首选」—— 建议：弃（大）

- **1.x 行为**：同一平台可添加多个 API（`apis[]` 数组，各带 `mode`/`billing_period`），
  可设「首选」，托盘菜单与看板围绕首选切换。
- **2.0 现状**：一平台一 key（`secure_settings` 按平台 key 存），余额页与托盘图标跟随
  DeepSeek（未配置 DeepSeek 时取第一个有读数的平台）。
- **取舍**：多账户会重写配置、密钥命名、侧边栏与历史归属；当前需求（一平台一账户）不覆盖成本。
  若确有需求，建议先只做「同一平台多 key 轮换」而不引入「首选」概念。

### 4.7 热力图与时段分布图 —— 建议：弃（中）｜**热力图已做**（2.1）

- **1.x 行为**：看板除折线外还有 GitHub 风格热力图（180 天/30 天）与时段分布柱状图。
- **现状**：**热力图已在 2.1 的桌面小工具里实现**——`widget/charts.rs` 画 12 周（84 天）× 7 天的
  额度活动图，数据来自契约里的 `daily`，可按「全部订阅」或单个订阅切换（`docs/WIDGET.md` §4.4）。
  与 1.x 的差别只是窗口（12 周 vs 180/30 天可选）与所在位置（小工具 vs 看板）。主程序的余额页与
  订阅页**没有**热力图，仍是折线。
- **仍未做**：**时段分布柱状图**——两条路径都没有，且没有任何数据源在按小时聚合。
- **取舍**：剩下的那一半属于展示丰富度；要做得先决定"按小时聚合"要不要进历史（现在只有按天）。

---

## 五、建议的处理顺序

1. **4.2 MiniMax 重试**（小、纯健壮性，顺手就做）
2. **4.1 OCGo 插值精化**（中等、1.x 的核心展示特性，用户能直接看出差别）
3. **4.4 / 4.5 两类提醒**（小，但需要产品口径：是否要更多告警类型）
4. **4.3 MiniMax 状态页**（需要先决定非 DeepSeek 状态页怎么放）
5. 4.6 建议维持不做，除非出现明确需求；4.7 只剩时段分布图，同样等需求
