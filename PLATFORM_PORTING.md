# 待移植的订阅平台

Python 版（`main` 分支的 `src/platforms/`）支持 **8 个平台家族、15 个可添加条目**；
Rust GUI 版目前只实现了其中三个（DeepSeek、OpenCode Go、Command Code）。

本文件记录其余平台的接口细节与实现要点，作为移植依据。所有内容来自 `main` 分支的
Python 实现，未做改动。

## 总览

| 家族 | 条目 | 指标类型 | 端点 | 移植难度 |
|---|---|---|---|---|
| Kimi | `kimi_token_cn`、`kimi_token_global` | 余额 | `api.moonshot.cn` / `api.moonshot.ai` | 低 |
| StepFun | `stepfun_token_cn`、`stepfun_token_global` | 余额 | `api.stepfun.com` / `api.stepfun.ai` | 低 |
| OpenRouter | `openrouter` | 余额（派生） | `openrouter.ai/api/v1` | 低 |
| MiniMax | `minimax_token_cn/global`、`minimax_coding_cn/global` | 额度窗口 | `minimaxi.com` / `minimax.io` | 中 |
| GLM Coding | `glm_coding_cn`、`glm_coding_global` | 额度窗口 | `open.bigmodel.cn` / `api.z.ai` | 中高 |

已完成：`deepseek`（余额）、`opencode_go`（额度窗口）、`command_code`（额度窗口）。

## 两类指标模型

Python 版把所有平台归为两类，界面按类别渲染不同的形态。

### payg —— 账户余额

返回形状（与 DeepSeek 客户端一致）：

```json
{
  "is_available": true,
  "all_balances": {
    "CNY": { "total_balance": 0.0, "granted_balance": 0.0, "topped_up_balance": 0.0 }
  }
}
```

### package —— 订阅额度窗口

返回形状：

```json
{
  "5h":      { "usage_percent": 0.0, "percent_remaining": 100.0, "reset_in_sec": 0 },
  "weekly":  null,
  "monthly": null
}
```

`percent_remaining` 是主指标；Rust 目前的 `CommandCodeWindow` 存的是 `used/cap`
绝对值，移植时需要统一约定或加适配层。

---

## Kimi（低）

| 项 | 值 |
|---|---|
| 端点 | CN `https://api.moonshot.cn/v1/users/me/balance`；Global `https://api.moonshot.ai/v1/users/me/balance` |
| 认证 | `Authorization: Bearer <key>` |
| 币种 | CN → CNY，Global → USD |

响应字段（`data` 内）：`available_balance`、`voucher_balance`、`cash_balance`。

映射：`total ← available_balance`，`granted ← voucher_balance`，`topped ← cash_balance`，
`is_available = available_balance > 0`。

校验：`code != 0` 或 `status` 为假时报错，错误信息带上 `scode`。
注意 **key 与域名绑定**——CN 的 key 打 Global 域名会得到 401。

## StepFun（低）

| 项 | 值 |
|---|---|
| 端点 | CN `https://api.stepfun.com/v1/accounts`；Global `https://api.stepfun.ai/v1/accounts` |
| 认证 | `Authorization: Bearer <key>` |
| 币种 | CN → CNY，Global → USD |

响应字段：`balance`、`total_cash_balance`、`total_voucher_balance`、`type`。

映射：`total ← balance`，`topped ← total_cash_balance`，`granted ← total_voucher_balance`；
`type` 缺省为 `prepaid`，随结果带出 `account_type`（`postpaid` 目前无语义）。

该端点**不覆盖 Step Plan 订阅**（官方没有公开配额 API），只能读到按量付费余额。

## OpenRouter（低）

| 项 | 值 |
|---|---|
| 端点 | `https://openrouter.ai/api/v1/credits` |
| 认证 | `Authorization: Bearer <key>`，**必须是 Management Key** |

响应字段在 `data` 内：`total_credits`、`total_usage`。

映射：`total ← total_credits − total_usage`（派生值），`granted ← 0`。

错误处理要点：**401 与 403 都要按"这不是 Management Key"提示**——普通推理 key
打这个端点会失败。返回的是账户级余额，不是单个 key 的额度。

## MiniMax（中）

四个条目共用两个端点，靠请求路径区分 Token Plan 与 Coding Plan：

| 条目 | 端点 |
|---|---|
| `minimax_token_cn` | `https://www.minimaxi.com/v1/token_plan/remains` |
| `minimax_token_global` | `https://www.minimax.io/v1/token_plan/remains` |
| `minimax_coding_cn` | `https://www.minimaxi.com/v1/api/openplatform/coding_plan/remains` |
| `minimax_coding_global` | `https://www.minimax.io/v1/api/openplatform/coding_plan/remains` |

认证：`Authorization: Bearer <key>`。窗口：**只有 5h 与 weekly，没有月度**。

解析要点（这几处最容易出错）：

- `model_remains` 数组可能在 `data.model_remains`，也可能直接在根级，两处都要找
- 优先取 `model_name == "general"` 的那条，没有则取第一条
- 字段是**剩余**百分比：`current_interval_remaining_percent`、`current_weekly_remaining_percent`；
  已用 = `100 − remaining`
- 时间戳 `end_time`、`weekly_end_time` 需要判断秒/毫秒（大于 `1e12` 视为毫秒）
- 响应头的 `base_resp.status_code` 为 0 才算成功
- Python 版做了 **3 次重试**（针对 URLError / SSLError / ConnectionError / TimeoutError，
  间隔 1 秒）并显式发送 `Connection: close`；Rust 侧可评估是否需要
- 另有一个可选的 `fetch_minimax_service_status()`（抓 `status.minimax.io` 的 HTML）

## GLM Coding（中高）

| 项 | 值 |
|---|---|
| 端点 | CN `https://open.bigmodel.cn/api/monitor/usage/quota/limit`；Global `https://api.z.ai/api/monitor/usage/quota/limit` |
| 认证 | `Authorization: Bearer <key>`，**401 时去掉 Bearer 前缀用裸 key 重试一次** |

响应结构：顶层 `code` 与 `success`，数据在 `data.limits[]`，每项有
`type`、`percentage`、`nextResetTime`。

解析要点：

- `code ∈ {0, 200}` 且 `success` 为真才算成功
- 窗口靠**数组出现顺序**识别：`TOKENS_LIMIT` 的第 0 项是 5h、第 1 项是 weekly；
  `TIME_LIMIT` 是 monthly。官方响应没有窗口名字段，这里最脆弱
- **monthly 是 MCP 工具调用次数**，不是 token 额度，与另外两个窗口语义不同却并排显示
- `nextResetTime` 是**毫秒**
- `percentage` 是"已用"，剩余 = `100 − percentage` 并夹到 `[0, 100]`

---

## 移植前需要先做的事

单个平台的 fetch 都不难，真正的成本在模型层——Rust 版目前**没有多平台概念**：

1. **配置**：`AppConfig` 只有单一 `api_key`。Python 用的是
   `apis: [{ id, name, platform, mode, billing_period }]` 多账户数组，
   每个条目选平台、填一个 key。移植前需要引入等价的模型。
2. **界面渲染**：现在硬编码三条额度条。需要改成"按平台类型决定渲染形态"——
   `payg` 渲染余额卡片，`package` 按该平台声明的窗口清单渲染窗口条。
3. **指标约定**：统一 `percent_remaining` 与 `used/cap` 的选择，避免每个平台各写一套。
4. **凭据命名**：`secure_settings` 里现在按平台写了固定键名
   （`opencode_go_api_key` 等）。多账户后需要按条目 id 存，或保持"一平台一键"的现状。

## 建议的移植顺序

1. **Kimi → StepFun → OpenRouter**：三个都是单请求 + 字段映射，可以一起做，
   顺便验证"多平台抽象"是否站得住
2. **MiniMax**：需要处理双端点、模型优选与时间戳判断
3. **GLM**：401 重试与位置识别的窗口解析最需要单独测试
