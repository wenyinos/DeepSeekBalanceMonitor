# 跨会话临时上下文文档

> 生成时间：2026-08-21，供后续冷启动恢复进度。

## 项目状态：v2.0 多平台多账号版本

### 已完成（本次会话）

1. **死代码清理** — 删除 `credential_store` 兼容层、`get_balance_history` 死函数、`mac/keystore` CommonCrypto、遗留 `currency/api_key_enc` 字段、多余 `import os`
2. **统一 mac → Windows 逻辑** — `mac/settings.py`、`mac/main.py`、`webview/bridge.py` 全部收敛到 `secure_settings` 单一密钥源
3. **构建修复** — 安装 `cryptography`，修复 `hook-cryptography` PyInstaller 打包缺失（导致 SQLite 加密静默失败 → 无限弹设置页）
4. **主窗口重构** — 原 `Toplevel` 独立窗口 → `src/main_window.py` 统一 `ttk.Notebook` 单例（`withdraw`/`deiconify`），标题 `DSMonitor`，`860x700`
5. **多账号基础（迭代1）** — `config.json: apis[] + preferred_api_id`、`secure_settings: api:{id}:key`、`balance_history.api_id` 列、`ApiManagementFrame` 增删改查、`HistoryFrame` 二级 API 选择、`SettingsFrame` 首选展示项、托盘 `🔀 API选择` 二级菜单、首次启动进 API 管理页
6. **历史数据延迟** — `HistoryFrame.on_show/refresh` 增加 timestamp 对比自动 `_reload()`，解决 `withdraw` 下 Tab 数据滞留

### 未完成（需继续）

1. **按量/套餐双模式 UI**
   - `ApiManagementFrame._open_form` 中 OpenCode Go 表单应改为**单 API Key 输入**（不再 workspace_id/cookie）
   - 设置页 `threshold_package_percent`（默认10%）与 `package_display_period`（5h/weekly/monthly）下拉

2. **套餐模式逻辑**
   - `tray_app.do_balance_check` 对 `mode=package` 调 `opencode_client.fetch_opencode_quota(api_key)` → `save_package_record`
   - `AppState` 新增 `package_data` 字段，托盘 tooltip 按 `package_display_period` 显示月剩百分比（如 `月剩余 68%`）
   - 套餐模式通知：`5h滚动：0%（4h 34m 后重置）\n每周：4%（3d 11h 后重置）\n每月：2%（29d 20h 后重置）`
   - 套餐模式不统计消耗速率，不检测 API 可用性
   - `threshold_package_percent` 双阈值：低于此值图标变红

3. **历史图表按模式分流**
   - `payg` → `balance_history` 折线（原有）
   - `package` → `package_history` 月剩百分比折线，时间轴显示 timestamp
   - `package_history` 表结构：`api_id, timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset`（已建表）

4. **设置页套餐控件**
   - `threshold_package_percent`：Spinbox 0-100
   - `package_display_period`：Combobox `5h / weekly / monthly`

5. **opencode_client 已实现**
   - 仅官方 API `GET /zen/go/v1/usage`（`Authorization: Bearer + x-api-key`），无抓取回退
   - 返回 `{"rolling": {"usage_percent": float, "reset_in_sec": int}, ...}`
   - `format_reset_short` 辅助函数

### 关键文件清单

| 文件 | 用途 |
|---|---|
| `src/config.py` | DEFAULT_CONFIG（含 `threshold_package_percent/package_display_period`）、多API CRUD、迁移 |
| `src/secure_settings.py` | `api:{id}:key`、`opencode_go:{id}:*` 加密存储 |
| `src/storage.py` | `balance_history`（按量）、`package_history`（套餐）双表 |
| `src/api_management_frame.py` | API 管理 Tab（增删改查 + 平台/模式选择） |
| `src/main_window.py` | 统一主窗 `DSMonitor`，Notebook 四 Tab |
| `src/history_dialog.py` | 历史 Tab（二级 API 选择 + 按模式分流图表） |
| `src/settings_dialog.py` | 设置 Tab（首选展示 + 双阈值 + 套餐周期） |
| `src/tray_app.py` | 托盘菜单 `🔀 API选择`、`do_balance_check` 多API轮询 |
| `src/opencode_client.py` | Opencode Go 官方 API + 抓取回退 |
| `src/app_state.py` | 共享状态（`_main_window`） |
| `src/rainmeter_server.py` | Rainmeter 本地接口（已按 preferred API 过滤） |

### API 模式数据结构

```json
// config.json
{
  "apis": [
    {"id": "41f29a05", "platform": "deepseek", "mode": "payg", "name": "DeepSeek-1", "created_at": "..."},
    {"id": "abc12345", "platform": "opencode_go", "mode": "package", "name": "OpenCode Go-1", "created_at": "..."}
  ],
  "preferred_api_id": "41f29a05",
  "threshold_yuan": 1.0,
  "threshold_package_percent": 10,
  "package_display_period": "monthly"
}
```

### Opencode 官方 API

```
GET https://opencode.ai/zen/go/v1/usage
Headers:
  Authorization: Bearer <Go API Key>
  x-api-key: <Go API Key>
  User-Agent: Mozilla/5.0 ...
Response:
{
  "useBalance": bool,
  "rollingUsage":  {"usagePercent": float, "resetInSec": int, "status": "ok"|"rate-limited"},
  "weeklyUsage":   {"usagePercent": float, "resetInSec": int, "status": ...},
  "monthlyUsage":  {"usagePercent": float, "resetInSec": int, "status": ...}
}
```

### 注意事项

- `API Key` 存 `secure_settings.db`，`config.json` 永远写空
- `credential_store.py` 已 stub，`mac/keystore.py` 已透传 `secure_settings`
- `ttkbootstrap` 尝试后用户评价"不如原来的"已回滚，保持原生 Tk
- PyInstaller 构建需 `cryptography` 在 `requirements.txt`
- 托盘 `查看余额` 保持 `pystray.notify` 通知，未并入主窗
- `WM_DELETE_WINDOW` 协议为 `withdraw` 而非 `destroy`
