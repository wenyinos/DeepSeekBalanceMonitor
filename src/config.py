"""
Constants, i18n, logging, config load/save, DPI awareness.
Imported by all other modules.
"""
import ctypes
import json
import os
import sys
from datetime import datetime
from pathlib import Path

# ─── High-DPI Awareness (before any GUI) ──────────────────────────
def _set_dpi_awareness():
    if sys.platform != "win32":
        return
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        try:
            ctypes.windll.user32.SetProcessDPIAware()
        except Exception:
            pass

_set_dpi_awareness()

# ─── Constants ────────────────────────────────────────────────────
APP_NAME = "DeepSeek Balance Monitor"
APP_ID   = "deepseek-balance-monitor"
CONFIG_DIR = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming")) / APP_NAME
CONFIG_FILE = CONFIG_DIR / "config.json"
LOG_FILE    = CONFIG_DIR / "app.log"

DEFAULT_CONFIG = {
    "api_key": "",
    "interval_minutes": 10,
    "threshold_yuan": 1.0,
    "language": "zh",
    "auto_start": False,
    "enable_alerts": True,
}

# ─── i18n ─────────────────────────────────────────────────────────
_T = {
    "zh": {
        "total_balance":    "总余额",
        "topped_up":        "充值余额",
        "granted":          "赠送余额",
        "currency":         "货币",
        "checking":         "查询中…",
        "last_check":       "上次查询",
        "not_checked":      "尚未查询",
        "error_no_key":     "未配置 API Key",
        "error_fetch":      "查询出错",
        "view_balance":     "📋 查看余额",
        "check_now":        "🔄 立即查询",
        "settings":         "⚙  设置…",
        "quit":             "❌ 退出",
        "settings_title":   "DeepSeek Balance Monitor — 设置",
        "api_key_label":    "DeepSeek API Key:",
        "show_key":         "显示 API Key",
        "interval_label":   "查询间隔（分钟）：",
        "interval_hint":    "  （1 ~ 1440 分钟）",
        "threshold_label":  "余额预警线：",
        "threshold_hint":   "  低于此值时托盘图标显示红色预警",
        "language_label":   "语言 / Language：",
        "save":             "保存",
        "cancel":           "取消",
        "warn_title":       "警告",
        "warn_no_key":      "API Key 不能为空！",
        "exit_no_key":      "请在下次启动时配置 API Key。程序退出。",
        "low_bal_title":    "⚠ DeepSeek 余额不足",
        "low_bal_msg":      "当前余额仅剩 {balance}，已低于您设置的提醒阈值 {threshold}。\n请及时充值！",
        "bal_error_title":  "DeepSeek 余额 — 错误",
        "bal_empty_title":  "DeepSeek 余额",
        "bal_empty_msg":    "尚未查询到余额，请稍后或点击「立即查询」",
        "bal_title":        "DeepSeek 余额: {balance}",
        "bal_msg":          "总余额:   {total}\n充值余额: {topped}\n赠送余额: {granted}\n货币:     {currency}\n上次查询: {time}",
        "tooltip_balance":  "总余额: {total} {code}",
        "tooltip_error":    "错误: {error}",
        "tooltip_checking": "查询中…",
        "status_line":      "上次查询: {last}  |  当前余额: {total} {code}",
        "status_line_no":   "上次查询: {last}",
        "bal_error_msg":    "查询出错: {error}",
        "bal_currency_line": "{code}: {total}  (充值 {topped}, 赠送 {granted})",
        "auto_start_label": "开机自动启动",
        "enable_alerts_label": "开启预警提醒",
    },
    "en": {
        "total_balance":    "Total Balance",
        "topped_up":        "Topped Up",
        "granted":          "Granted",
        "currency":         "Currency",
        "checking":         "Checking…",
        "last_check":       "Last Check",
        "not_checked":      "Not checked",
        "error_no_key":     "No API Key configured",
        "error_fetch":      "Fetch Error",
        "view_balance":     "📋 View Balance",
        "check_now":        "🔄 Check Now",
        "settings":         "⚙  Settings…",
        "quit":             "❌ Quit",
        "settings_title":   "DeepSeek Balance Monitor — Settings",
        "api_key_label":    "DeepSeek API Key:",
        "show_key":         "Show API Key",
        "interval_label":   "Check Interval (min):",
        "interval_hint":    "  (1 ~ 1440 min)",
        "threshold_label":  "Low Balance Threshold:",
        "threshold_hint":   "  Icon turns red when balance drops below this value",
        "language_label":   "Language / 语言：",
        "save":             "Save",
        "cancel":           "Cancel",
        "warn_title":       "Warning",
        "warn_no_key":      "API Key cannot be empty!",
        "exit_no_key":      "Please configure an API Key on next launch. Exiting.",
        "low_bal_title":    "⚠ DeepSeek Low Balance",
        "low_bal_msg":      "Balance is only {balance}, below your alert threshold of {threshold}.\nPlease top up!",
        "bal_error_title":  "DeepSeek Balance — Error",
        "bal_empty_title":  "DeepSeek Balance",
        "bal_empty_msg":    "No balance data yet. Please wait or click 'Check Now'.",
        "bal_title":        "DeepSeek Balance: {balance}",
        "bal_msg":          "Total:    {total}\nTopped Up: {topped}\nGranted:   {granted}\nCurrency:  {currency}\nLast Check: {time}",
        "tooltip_balance":  "Balance: {total} {code}",
        "tooltip_error":    "Error: {error}",
        "tooltip_checking": "Checking…",
        "status_line":      "Last: {last}  |  Balance: {total} {code}",
        "status_line_no":   "Last: {last}",
        "bal_error_msg":    "Fetch error: {error}",
        "bal_currency_line": "{code}: {total}  (Topped {topped}, Granted {granted})",
        "auto_start_label": "Auto-start on boot",
        "enable_alerts_label": "Enable balance alerts",
    },
}

def T(key: str, lang: str = "zh", **kwargs) -> str:
    table = _T.get(lang, _T["zh"])
    text = table.get(key)
    if text is None:
        text = _T["zh"].get(key, key)
    return text.format(**kwargs) if kwargs else text

# ─── Logging ─────────────────────────────────────────────────────
def log(msg: str):
    try:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(f"[{ts}] {msg}\n")
    except Exception:
        pass

# ─── Config I/O ──────────────────────────────────────────────────
def load_config() -> dict:
    if CONFIG_FILE.exists():
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                return {**DEFAULT_CONFIG, **json.load(f)}
        except Exception as e:
            log(f"Failed to load config: {e}")
    return DEFAULT_CONFIG.copy()

def save_config(config: dict) -> None:
    try:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(config, f, indent=2, ensure_ascii=False)
        log("Config saved")
    except Exception as e:
        log(f"Failed to save config: {e}")
