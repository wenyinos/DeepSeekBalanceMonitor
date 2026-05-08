"""
Application state - holds balances, config, timer, and helper methods.
"""
import sys
import threading
from datetime import datetime

from src.config import load_config, T, log, APP_ID


class AppState:
    def __init__(self):
        self.config = load_config()
        self.icon = None
        self.balances = {}
        self.last_check = None
        self.error = None
        self._timer = None
        self.running = True
        self._lock = threading.Lock()
        self._settings_open = False
        self._settings_window = None

    @property
    def lang(self):
        return self.config.get("language", "zh")

    def get_preferred_balance(self):
        for c, b in self.balances.items():
            return {**b, "currency": c}
        return None

    def balance_tooltip(self):
        with self._lock:
            if self.error:
                return T("tooltip_error", self.lang, error=self.error)
            b = self.get_preferred_balance()
            if b:
                return T("tooltip_balance", self.lang,
                         total=f"{b['total_balance']:,.2f}",
                         code=b["currency"])
            return T("tooltip_checking", self.lang)

    def is_low_balance(self):
        with self._lock:
            b = self.get_preferred_balance()
            if b is None:
                return False
            t = float(self.config.get("threshold_yuan", 1.0))
            return b["total_balance"] < t

    def is_ok(self):
        with self._lock:
            if self.error:
                return False
            b = self.get_preferred_balance()
            if b is None:
                return False
            t = float(self.config.get("threshold_yuan", 1.0))
            return b["total_balance"] >= t

    def schedule_next_check(self, cb, interval_sec):
        with self._lock:
            if self._timer:
                self._timer.cancel()
            if not self.running:
                return
            self._timer = threading.Timer(interval_sec, cb)
            self._timer.daemon = True
            self._timer.start()

    def cancel_timer(self):
        with self._lock:
            if self._timer:
                self._timer.cancel()
                self._timer = None


_RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"


def get_auto_start_state():
    if sys.platform != "win32":
        return False
    try:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, _RUN_KEY,
                           0, winreg.KEY_READ) as key:
            value, _ = winreg.QueryValueEx(key, APP_ID)
            exe_path = sys.executable
            return value == exe_path
    except (FileNotFoundError, OSError):
        return False


def set_auto_start(enable):
    if sys.platform != "win32":
        return
    exe_path = sys.executable
    try:
        import winreg
        if enable:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, _RUN_KEY,
                               0, winreg.KEY_SET_VALUE) as key:
                winreg.SetValueEx(key, APP_ID, 0, winreg.REG_SZ, exe_path)
            log(f"Auto-start enabled: {exe_path}")
        else:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, _RUN_KEY,
                               0, winreg.KEY_SET_VALUE) as key:
                winreg.DeleteValue(key, APP_ID)
            log("Auto-start disabled")
    except FileNotFoundError:
        if enable:
            with winreg.CreateKey(winreg.HKEY_CURRENT_USER, _RUN_KEY) as key:
                winreg.SetValueEx(key, APP_ID, 0, winreg.REG_SZ, exe_path)
            log(f"Auto-start enabled (key created): {exe_path}")
    except OSError:
        pass
