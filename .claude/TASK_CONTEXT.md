# 跨会话临时上下文文档

> 更新时间：2026-08-21 16:10，供后续冷启动恢复进度。

## 项目状态：v2.0 多平台多账号 + 双模式（按量/套餐）已实现

### 已完成（本次会话全量）

#### 1. 多平台注册表 `src/platforms.py`
- `PlatformMeta` 字段：`key/display_name/default_mode/package_windows/has_status_page/console_url`
- 已注册：`deepseek`(payg)、`opencode_go`(package, 5h+weekly+monthly)、`minimax_token_cn/global`、`minimax_coding_cn/global`(均为 package, 5h+weekly 无 monthly)
- **添加新平台只需在 PLATFORMS 字典加一行**

#### 2. API 管理 `src/api_management_frame.py`
- 添加/编辑弹窗：平台选择（4 MiniMax + 2 经典）、名称预填 `平台-序号`、API Key（编辑时显示 `已加密存储` placeholder）
- **展示周期**（`billing_period`）：仅纯套餐平台显示，`5h / 每周 / 每月`，标签中文汉化，选项来自 `platform.package_windows` 动态取值
- 树列表含 `展示周期` 列

#### 3. 按量/套餐双模式
- `config.json: apis[].mode` — `payg`/`package`
- `config.json: apis[].billing_period` — per-API，驱动托盘图标/通知/速率/图表的展示窗口
- 全局 `package_display_period` 已移除，改为 per-API
- `threshold_package_percent` 保留在全局设置中（套餐模式预警阈值）

#### 4. 托盘与通知
- **并行查询**：`ThreadPoolExecutor` 轮询所有 API（payg + package），各自写入历史
- **per-API 缓存**：`AppState._api_cache`，切换 API 时立即显示缓存，不立即重查，等正常轮询更新
- **套餐通知格式**：
  ```
  {api_name} 余额：
  5h滚动：剩余 96%（3小时 28分后重置）
  每周：剩余 93%（2天 19小时后重置）
  每月：剩余 97%（29天 5小时后重置）
  📡 API 服务状态：🟢 服务正常
  📊 忙时消耗 0.42%月额度/小时 | 预计可用忙时 15.0 小时
  🕐 上次查询：刚刚
  ```
- **图标**：payg 显示余额数字，package 按 `billing_period` 显示剩余%

#### 5. 服务状态
- `DeepSeek` → `fetch_service_status()`（FlashDuty）
- `MiniMax` → `fetch_minimax_service_status()`（`status.minimax.io`，匹配 "Large Language Models" 组件）
- `OpenCode Go` → 无状态页，不获取
- 按 preferred API 平台分发，存入 `AppState._api_cache[api_id].service_status`

#### 6. MiniMax API `src/minimax_client.py`
- `minimaxi.com`（国内）/ `minimax.io`（国际）
- Token Plan: `/v1/token_plan/remains`，Coding Plan: `/v1/api/openplatform/coding_plan/remains`
- 响应格式：`{"data":{"model_remains":[{"model_name","current_interval_remaining_percent","current_weekly_remaining_percent",...}]}}`
- 取 `general` 模型，解析 5h + weekly 剩余%和重置秒数

#### 7. OCGo API `src/opencode_client.py`
- **仅官方接口**，无抓取回退：`GET /zen/go/v1/usage`，`Authorization: Bearer + x-api-key`
- 响应：`{"usage":{"rolling":{"usagePercent","resetInSec"}, "weekly":{...}, "monthly":{...}}}`

#### 8. 历史表 schema
| 模式 | 表 | 列 |
|---|---|---|
| payg（DeepSeek） | `balance_history` | `api_id, timestamp, currency, total, topped, granted, service_status` |
| package（OCGo） | `package_history` | `api_id, timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset, service_status` |
| package（MiniMax） | `package_history` | 同上，但无 monthly 数据（5h+weekly 两档） |

- `package_history.service_status` 列通过 `ALTER TABLE` 迁移添加（旧表兼容）
- `save_package_record` 接受 `service_status` 参数

#### 9. 树列动态化 `src/history_dialog.py`
- `package_windows` 字段决定展示列：OCGo `[5h,weekly,monthly]`，MiniMax `[5h,weekly]`
- `has_status_page` 字段决定是否显示状态列：DeepSeek/MiniMax 有，OCGo 无
- `_on_api_selected` 在 `_build` 末尾调用，确保首次打开即显示正确 schema

#### 10. 主窗口 `src/main_window.py`
- 标题 `DSMonitor`，`860x700`，History/Settings/Dev tabs
- `WM_DELETE_WINDOW → withdraw`（常驻不销毁）
- 设置保存后关闭窗口，不触发重查（用缓存数据）
- 切 Tab 时检查设置未保存状态（`SettingsFrame._dirty`）

### 关键文件清单

| 文件 | 用途 |
|---|---|
| `src/platforms.py` | 平台注册表（6 平台），含 `package_windows/has_status_page/console_url` |
| `src/config.py` | DEFAULT_CONFIG、per-API `billing_period`、`apis[]/preferred_api_id`、CRUD |
| `src/secure_settings.py` | `api:{id}:key` 加密存储 |
| `src/storage.py` | `balance_history` + `package_history` 双表，`service_status` 迁移 |
| `src/api_management_frame.py` | API 管理 Tab（增删改查、展示周期、Key 占位符） |
| `src/main_window.py` | 统一主窗 `DSMonitor`，History/Settings/Dev tabs |
| `src/history_dialog.py` | 历史 Tab（API 二级选择、动态列、billing_period 图表、per-API 状态） |
| `src/settings_dialog.py` | 设置 Tab（首选展示项 + 双阈值） |
| `src/tray_app.py` | 多API并行轮询、per-API缓存、generation防老化、`🔀 API选择` 菜单 |
| `src/minimax_client.py` | MiniMax Token/Coding Plan × CN/Global 官方接口 |
| `src/opencode_client.py` | OCGo 官方接口（仅 `/zen/go/v1/usage`，无抓取） |
| `src/api_client.py` | DeepSeek + MiniMax 状态页获取 |
| `src/app_state.py` | `_api_cache`/`_check_generation`/`package_data` |
| `src/icon_renderer.py` | 图标按 `billing_period` 显示 package 剩余% |

### config.json schema

```json
{
  "apis": [
    {"id":"41f29a05","platform":"deepseek","mode":"payg","billing_period":"","name":"DeepSeek-1","created_at":"..."},
    {"id":"c9f893cc","platform":"opencode_go","mode":"package","billing_period":"monthly","name":"OpenCode Go-1","created_at":"..."},
    {"id":"5a7571cb","platform":"minimax_token_cn","mode":"package","billing_period":"weekly","name":"MiniMax Token Plan (CN)-1","created_at":"..."}
  ],
  "preferred_api_id": "41f29a05",
  "threshold_yuan": 1.0,
  "threshold_package_percent": 10,
  "interval_minutes": 10,
  "language": "zh"
}
```

### 注意事项

- `API Key` 存 `secure_settings.db`，`config.json` 永远写空
- `credential_store.py` / `mac/keystore.py` 已 stub 透传
- PyInstaller 构建需 `cryptography` 在 `requirements.txt`
- 托盘 `查看余额` 保持 `pystray.notify` 通知，未并入主窗
- `WM_DELETE_WINDOW` 协议为 `withdraw`，主窗常驻
- API 切换时立即显示缓存数据，不立即重查，等正常轮询更新
- 设置保存不触发重查，只刷新图标/菜单
- `generation_counter` 防止旧线程覆盖新数据
