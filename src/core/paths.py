"""
Shared constants and logging — imported by config, secure_settings, storage.
No dependencies on other src modules (leaf node).
"""
import sys
from pathlib import Path

APP_NAME = "DeepSeek Balance Monitor"
APP_ID   = "deepseek-balance-monitor"

if sys.platform == "darwin":
    CONFIG_DIR = Path.home() / "Library" / "Application Support" / APP_NAME
else:
    import os
    CONFIG_DIR = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming")) / APP_NAME

CONFIG_FILE = CONFIG_DIR / "config.json"
LOG_FILE    = CONFIG_DIR / "app.log"
DB_FILE     = CONFIG_DIR / "balance_history.db"


def log(msg: str):
    try:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        from datetime import datetime
        ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(f"[{ts}] {msg}\n")
    except Exception:
        pass
