"""
Tray application — balance checking loop, notifications, tray menu, and entry point.
"""
import sys
import threading
from datetime import datetime

import pystray

from src.config import T, log, CONFIG_DIR, APP_NAME, APP_ID
from src.api_client import fetch_balance
from src.icon_renderer import create_icon_image
from src.app_state import AppState


# pystray on Windows uses Shell_NotifyIconA whose NOTIFYICONDATA.szTip / szInfo
# fields are ANSI (code-page dependent).  On Chinese Windows the system code page
# is GBK which handles Chinese natively — but any character outside the current
# code page will raise UnicodeEncodeError at the ctypes boundary.
# We only sanitise the *exception message* (which may contain arbitrary Unicode
# from API error bodies) before it reaches a tooltip or notification.
def _sanitise_error(text):
    """Strip characters that cannot be encoded in the system ANSI code page."""
    if text is None:
        return ""
    try:
        text.encode("mbcs")
        return text
    except (UnicodeEncodeError, LookupError):
        return text.encode("mbcs", errors="replace").decode("mbcs")


# --- Balance Check --------------------------------------------------

def do_balance_check(app: AppState):
    api_key = app.config.get("api_key", "").strip()
    if not api_key:
        with app._lock:
            app.error = T("error_no_key", app.lang)
            app.balances = {}
    else:
        try:
            data = fetch_balance(api_key)
            with app._lock:
                app.balances = data["all_balances"]
                app.error = None
                app.last_check = datetime.now()
            b = app.get_preferred_balance()
            if b:
                log(f"Balance OK: {b['total_balance']:.2f} {b['currency']}")
        except Exception as e:
            # Only the exception text is risky — API error bodies may carry
            # characters outside the system ANSI code page.
            raw = str(e).split("\n")[0]
            with app._lock:
                app.error = _sanitise_error(raw)
                app.balances = {}
            log(f"Check failed: {e}")

    if app.icon:
        app.icon.title = app.balance_tooltip()
        app.icon.icon = create_icon_image(app)

    if app.is_low_balance() and app.config.get("enable_alerts", True):
        notify_user(app)

    interval_sec = int(app.config.get("interval_minutes", 10)) * 60
    app.schedule_next_check(lambda: do_balance_check(app), interval_sec)


# --- Low-Balance Notification ---------------------------------------

def notify_user(app: AppState):
    b = app.get_preferred_balance()
    if b is None:
        return
    lang = app.lang
    t = app.config.get("threshold_yuan", 1.0)
    code = b["currency"]
    bal_str = f"{b['total_balance']:,.2f} {code}"
    thr_str = f"{t:,.2f} {code}"
    title = T("low_bal_title", lang)
    msg = T("low_bal_msg", lang, balance=bal_str, threshold=thr_str)
    try:
        app.icon.notify(msg, title=title)
        log(f"Notification sent: {b['total_balance']:.2f}")
    except Exception as e:
        log(f"Notification failed: {e}")
        alert_file = CONFIG_DIR / "LOW_BALANCE_ALERT.txt"
        try:
            with open(alert_file, "w", encoding="utf-8") as f:
                f.write(f"{title}\n\n{msg}\n")
        except Exception:
            pass


# --- Tray Menu Actions ----------------------------------------------

def on_show_balance(icon, item):
    app = getattr(icon, "_app", None)
    if app is None:
        return
    lang = app.lang
    with app._lock:
        balances = dict(app.balances)
        err = app.error
        last = app.last_check

    if err:
        title = T("bal_error_title", lang)
        msg = T("bal_error_msg", lang, error=err)
    elif not balances:
        title = T("bal_empty_title", lang)
        msg = T("bal_empty_msg", lang)
    else:
        time_str = last.strftime("%Y-%m-%d %H:%M:%S") if last else "-"
        lines = []
        for code, b in balances.items():
            lines.append(T("bal_currency_line", lang,
                           code=code,
                           total=f"{b['total_balance']:,.2f}",
                           topped=f"{b['topped_up_balance']:,.2f}",
                           granted=f"{b['granted_balance']:,.2f}"))
        msg = "\n".join(lines)
        if last:
            msg += f"\n{T('last_check', lang)}: {time_str}"

        pb = app.get_preferred_balance()
        if pb:
            title = T("bal_title", lang,
                      balance=f"{pb['total_balance']:,.2f} {pb['currency']}")
        else:
            first_code = next(iter(balances))
            title = T("bal_title", lang,
                      balance=f"{balances[first_code]['total_balance']:,.2f} {first_code}")

    try:
        icon.notify(msg, title=title)
    except Exception as e:
        log(f"Show-balance notify failed: {e}")


def on_check_now(icon, item):
    app = getattr(icon, "_app", None)
    if app is None:
        return
    app.cancel_timer()
    threading.Thread(target=do_balance_check, args=(app,), daemon=True).start()
    log("Manual check triggered")


def on_settings(icon, item):
    app = getattr(icon, "_app", None)
    if app is None:
        return
    try:
        from src.settings_dialog import open_settings
        open_settings(app)
    except Exception as e:
        log(f"Settings error: {e}")


def on_quit(icon, item):
    app = getattr(icon, "_app", None)
    if app is None:
        icon.stop()
        return
    app.running = False
    app.cancel_timer()
    log("Shutting down")
    icon.stop()


def make_menu(app: AppState):
    lang = app.lang
    return pystray.Menu(
        pystray.MenuItem(T("view_balance", lang), on_show_balance, default=True),
        pystray.MenuItem(T("check_now", lang), on_check_now),
        pystray.MenuItem(T("settings", lang), on_settings),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem(T("quit", lang), on_quit),
    )


# --- Entry Point ----------------------------------------------------

def main():
    log("=" * 50)
    log(f"{APP_NAME} starting")

    app = AppState()

    # First run -- force settings if no API key
    if not app.config.get("api_key", "").strip():
        log("No API key -- opening settings")
        try:
            from src.settings_dialog import open_settings
            open_settings(app)
            app = AppState()
        except Exception as e:
            log(f"Settings failed: {e}")

        if not app.config.get("api_key", "").strip():
            log("No API key provided -- exiting")
            print(T("exit_no_key", app.config.get("language", "zh")))
            sys.exit(0)

    # Create tray icon
    icon_img = create_icon_image(app)
    app.icon = pystray.Icon(
        APP_ID,
        icon_img,
        title=app.balance_tooltip(),
        menu=make_menu(app),
    )
    app.icon._app = app

    # Start first balance check
    threading.Thread(target=do_balance_check, args=(app,), daemon=True).start()
    log("First balance check scheduled")

    try:
        app.icon.run()
    except KeyboardInterrupt:
        pass
    finally:
        app.running = False
        app.cancel_timer()
        log("Exited cleanly")
