"""
Main window — unified Notebook with History / Settings / Dev tabs.
Windows-only; mac keeps its own webview/tk fallback.
Single instance: tray clicks always reuse the same Toplevel and select the requested tab.
"""
import os
import sys
import tkinter as tk
from tkinter import ttk

from src.core.config import T, log


class MainWindow:
    def __init__(self, app):
        self.app = app
        self._win = None
        self._notebook = None
        self._tabs = {}

    def _ensure(self):
        if self._win is not None and self._win.winfo_exists():
            return self._win
        root = self.app._tk_root
        win = tk.Toplevel(root)
        self._win = win
        # keep MainWindow instance in app._main_window (set by caller), don't overwrite with Toplevel
        if getattr(self.app, "_main_window", None) is None or isinstance(self.app._main_window, tk.Toplevel):
            self.app._main_window = self
        # keep legacy flags in sync for callers that check them
        self.app._settings_window = win
        self.app._history_window = win

        lang = self.app.lang
        win.title("DSMonitor")
        # Height computed to show EXACTLY two chart blocks + the third block's header.
        # Chart canvases are fixed 210 physical px; headers/paddings scale with DPI
        # (tkk/ttk fonts scale automatically), so only those parts multiply by _scale.
        try:
            _scale = win.winfo_fpixels("1i") / 96.0
        except Exception:
            _scale = 1.0
        _need_h = int((28 + 125 + 10) * _scale + 2 * (30 * _scale + 210) + 30 * _scale + 40)
        win.geometry(f"860x{_need_h}")
        win.minsize(600, 500)
        # center
        win.update_idletasks()
        sw, sh = win.winfo_screenwidth(), win.winfo_screenheight()
        w, h = win.winfo_width(), win.winfo_height()
        win.geometry(f"+{(sw - w)//2}+{(sh - h)//2}")

        try:
            if getattr(sys, "frozen", False):
                icon_path = os.path.join(sys._MEIPASS, "app.ico")
            else:
                # src/ui/main_window.py → repo root = parents[2]
                icon_path = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "assets", "app.ico")
            if os.path.isfile(icon_path):
                win.iconbitmap(icon_path)
        except Exception:
            pass

        # hide on close, with unsaved check
        self._last_tab = "settings"
        def _on_close():
            try:
                sett = self._tabs.get("settings")
                if sett and hasattr(sett, "check_unsaved") and not sett.check_unsaved():
                    return  # user chose not to discard
            except Exception:
                pass  # don't block close on check failure
            try:
                win.withdraw()
            except Exception:
                pass
        win.protocol("WM_DELETE_WINDOW", _on_close)

        nb = ttk.Notebook(win)
        self._notebook = nb
        nb.pack(fill="both", expand=True, padx=6, pady=6)

        # Lazy tab construction: pre-register lightweight holders (keeps tab order
        # stable); each tab's real content builds on FIRST selection — first launch
        # only pays for the initially visible tab, killing the startup jank.
        self._holders = {}    # key -> placeholder frame inside notebook
        self._builders = {}   # key -> callable(parent) -> content widget

        def _register(key, title, builder):
            holder = ttk.Frame(nb)
            nb.add(holder, text=title)
            self._holders[key] = holder
            self._builders[key] = builder

        def _build_manage(parent):
            from src.ui.manage_frame import ManageFrame
            return ManageFrame(parent, self.app, on_change=self._on_api_change)

        def _build_dashboard(parent):
            from src.ui.history_dialog import HistoryFrame
            return HistoryFrame(parent, self.app)

        def _build_settings(parent):
            from src.ui.settings_dialog import SettingsFrame
            return SettingsFrame(parent, self.app, on_save=self._on_settings_saved)

        _register("manage", T("manage", lang), _build_manage)
        _register("history", T("dashboard", lang), _build_dashboard)
        _register("settings", T("settings", lang).rstrip("…"), _build_settings)
        if self.app.demo_mode:
            def _build_dev(parent):
                from src.tray_app import DevFrame
                return DevFrame(parent, self.app)
            _register("dev", T("dev_tools", lang), _build_dev)

        nb.bind("<<NotebookTabChanged>>", self._on_tab_changed)

        # start hidden
        win.withdraw()
        return win

        # start hidden
        win.withdraw()
        return win

    def _ensure_tab(self, key):
        """Build a registered tab's content on first use. Returns content or None."""
        if key in self._tabs:
            return self._tabs[key]
        builder = self._builders.get(key)
        holder = self._holders.get(key)
        if not builder or not holder:
            return None
        try:
            w = builder(holder)
            w.pack(fill="both", expand=True)
            self._tabs[key] = w
            return w
        except Exception as e:
            log(f"Failed to build {key} tab: {e}")
            return None

    def _on_tab_changed(self, e):
        """NotebookTabChanged: lazy-build target tab, then on_show/refresh."""
        try:
            sel = self._notebook.select()
            holder = self._notebook.nametowidget(sel)
            new_tab = None
            for k, h in self._holders.items():
                if h is holder:
                    new_tab = k
                    break
            self._last_tab = new_tab
            w = self._ensure_tab(new_tab)
            if w is not None:
                if hasattr(w, "on_show"):
                    w.on_show()
                if hasattr(w, "refresh"):
                    w.refresh()
        except Exception as e:
            log(f"Tab change failed: {e}")

    def show(self, tab="manage"):
        win = self._ensure()
        # tray items map directly to tab keys; unknown → first tab
        tab_map = {"api_management": "manage", "manage": "manage", "history": "history", "dashboard": "history", "ledger": "manage", "settings": "settings", "dev": "dev", "apis": "manage"}
        key = tab_map.get(tab, tab)
        if key not in self._holders:
            # fallback to first registered tab
            key = next(iter(self._holders), None)
        if key:
            try:
                w = self._ensure_tab(key)
                self._notebook.select(self._holders[key])
                if w is not None:
                    if hasattr(w, "on_show"):
                        w.on_show()
                    if hasattr(w, "refresh"):
                        w.refresh()
            except Exception:
                pass
        try:
            win.deiconify()
            win.lift()
            win.after(50, win.focus_force)
        except Exception:
            pass
        # deterministically pre-build remaining tabs AFTER first paint — but one tab
        # per tick (chained), so no single callback blocks the UI long enough to jank
        def _prebuild_next():
            for k in self._holders:
                if k not in self._tabs:
                    self._ensure_tab(k)
                    try:
                        w = self._tabs.get(k)
                        if w is not None and hasattr(w, "on_show"):
                            w.on_show()
                    except Exception:
                        pass
                    # schedule the NEXT tab build on a later tick
                    win.after(350, _prebuild_next)
                    return
        win.after(200, _prebuild_next)

    def _leave_settings_check(self):
        """If settings has unsaved changes, prompt save/discard.
        Returns False when the user cancels (stay on settings)."""
        sett = self._tabs.get("settings")
        if not sett or not hasattr(sett, "check_unsaved"):
            return True
        try:
            if getattr(sett, "_dirty", False):
                return sett.check_unsaved()
        except Exception:
            pass
        return True

    def hide(self):
        if not self._leave_settings_check():
            return  # user cancelled — stay on settings, keep window open
        if self._win and self._win.winfo_exists():
            try:
                self._win.withdraw()
            except Exception:
                pass

    def close_for_rebuild(self):
        """Tear down the window + all lazy tab state so the next show() rebuilds
        from scratch (used after a language switch — widgets can't re-i18n live)."""
        try:
            if self._win and self._win.winfo_exists():
                self._win.destroy()
        except Exception:
            pass
        self._win = None
        self._tabs.clear()
        self._holders.clear()
        self._builders.clear()

    def _on_api_change(self):
        # called when APIs added/edited/deleted — refresh OTHER tabs.
        # NOTE: do not touch manage.mgmt here — mgmt.refresh() invokes on_change,
        # which would recurse (mgmt → on_change → mgmt → …)
        try:
            hist = self._tabs.get("history")
            if hist and hasattr(hist, "refresh_api_selector"):
                hist.refresh_api_selector()
        except Exception:
            pass
        try:
            sett = self._tabs.get("settings")
            if sett and hasattr(sett, "refresh_preferred_selector"):
                sett.refresh_preferred_selector()
        except Exception:
            pass
        try:
            from src.ui.icon_renderer import create_icon_image
            if self.app.icon:
                self.app.icon.icon = create_icon_image(self.app)
                if hasattr(self.app, "_rebuild_menu"):
                    self.app.icon.menu = self.app._rebuild_menu()
        except Exception:
            pass

    def _on_settings_saved(self):
        # called from SettingsFrame after successful save — refresh tray and history
        try:
            from src.ui.icon_renderer import create_icon_image
            if self.app.icon:
                self.app.icon.icon = create_icon_image(self.app)
                if hasattr(self.app, "_rebuild_menu"):
                    self.app.icon.menu = self.app._rebuild_menu()
        except Exception:
            pass
        # also refresh api management and history selectors (preferred may have changed)
        try:
            self._on_api_change()
        except Exception:
            pass
        # dashboard should follow the newly-saved preferred API
        try:
            self.refresh_all(follow_preferred=True)
        except Exception:
            pass

    def refresh_all(self, follow_preferred=False):
        for w in self._tabs.values():
            try:
                if hasattr(w, "refresh"):
                    w.refresh(follow_preferred=follow_preferred)
            except Exception:
                pass
