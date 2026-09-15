# 文档索引

本目录放文档。仓库根目录只保留约定俗成的几份：`README.md` / `README_zh.md`（面向使用者）、
`AGENTS.md`（agent 易踩坑的高信号事实）、`CLAUDE.md`（项目权威细节）、`CONTRIBUTING.md`。

| 文档 | 内容 | 什么时候看 |
|---|---|---|
| [`INTERFACES.md`](INTERFACES.md) | **Rust 与 Python 版应共同暴露的接口清单**：本地数据接口、配置文件、状态目录与数据库、单实例与唤出、自启、平台目录 | 写第二个实现、或改动任何对外可见的行为之前 |
| [`PLATFORM_PORTING.md`](PLATFORM_PORTING.md) | 各订阅平台的接口细节（端点、字段、易错点） | 加平台或改 API 客户端时 |
| [`WIDGET.md`](WIDGET.md) | 桌面小工具：设计规则与实现说明（含 7 张预览图；**已实现**，2.1 起随主程序发布） | 改小工具时；接口部分以 `INTERFACES.md` 为准 |
| [`GAPS.md`](GAPS.md) | 1.x 与 2.0 的差异、尚未移植的功能与建议处置 | 想知道「1.x 有的东西哪去了」时 |
| [`CODE_SIGNING.md`](CODE_SIGNING.md) | Windows 代码签名（SignPath）与发布流程 | 改发布流程或签名配置时 |
| [`CHANGELOG.md`](CHANGELOG.md) / [`CHANGELOG_zh.md`](CHANGELOG_zh.md) | 版本历史（英 / 中） | 想知道某个版本改了什么时 |

约定：

- 文档只描述**当前仓库**的真实行为，路径与常量以代码为准（例如 `dsmon-core/src/paths.rs` 是文件位置的
  唯一来源）。文档与代码不一致时，改文档。
- 涉及用户数据的说明（目录、密钥、数据库）必须写明**只读边界**：哪一份是只读的、哪一份会被搬移，
  以及搬移对「目标已存在」的处理。
- 预览图放在根目录的 `assets/`，本目录的文档用 `../assets/...` 引用。
