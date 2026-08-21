"""
Main window — unified Notebook with History / Settings / Dev tabs.
Windows-only; mac keeps its own webview/tk fallback.
Single instance: tray clicks always reuse the same Toplevel and select the requested tab.
"""
import os
import sys
import tkinter as tk
from tkinter import ttk

from src.config import T, log


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
        # History was 850x640, add tab height
        win.geometry("860x700")
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
                icon_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets", "app.ico")
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

        # API Management (first tab, for multi-account)
        from src.api_management_frame import ApiManagementFrame
        api_mgmt = ApiManagementFrame(nb, self.app, on_change=self._on_api_change)
        nb.add(api_mgmt, text="🔀 " + T("api_management", lang))
        self._tabs["api_management"] = api_mgmt

        # History (second tab, with second-level API selector)
        from src.history_dialog import HistoryFrame
        hist = HistoryFrame(nb, self.app)
        nb.add(hist, text=T("history", lang))
        self._tabs["history"] = hist

        # Settings
        from src.settings_dialog import SettingsFrame
        sett = SettingsFrame(nb, self.app, on_save=self._on_settings_saved)
        nb.add(sett, text=T("settings", lang).rstrip("…"))
        self._tabs["settings"] = sett

        # Dev (demo only)
        if self.app.demo_mode:
            from src.tray_app import DevFrame
            dev = DevFrame(nb, self.app)
            nb.add(dev, text=T("dev_tools", lang))
            self._tabs["dev"] = dev

        # redraw history chart when tab selected; check unsaved on leave settings
        def _on_tab_changed(e):
            try:
                sel = nb.select()
                widget = nb.nametowidget(sel)
                # determine which tab we're switching TO
                new_tab = None
                for k, w in self._tabs.items():
                    if w is widget:
                        new_tab = k
                        break
                # if leaving settings tab, check unsaved
                if self._last_tab == "settings" and new_tab != "settings":
                    sett = self._tabs.get("settings")
                    if sett and hasattr(sett, "check_unsaved"):
                        if not sett.check_unsaved():
                            # revert to settings tab
                            nb.select(self._tabs["settings"])
                            return
                self._last_tab = new_tab
                if hasattr(widget, "on_show"):
                    widget.on_show()
                if hasattr(widget, "refresh"):
                    widget.refresh()
            except Exception:
                pass
        nb.bind("<<NotebookTabChanged>>", _on_tab_changed)

        # start hidden
        win.withdraw()
        return win

    def show(self, tab="api_management"):
        win = self._ensure()
        # tray items map directly to tab keys; unknown → first tab
        tab_map = {"api_management": "api_management", "history": "history", "settings": "settings", "dev": "dev", "apis": "api_management"}
        key = tab_map.get(tab, tab)
        if key not in self._tabs:
            # fallback to first available tab
            key = next(iter(self._tabs), None)
        if key in self._tabs:
            try:
                self._notebook.select(self._tabs[key])
                # trigger on_show for that tab
                w = self._tabs[key]
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

    def hide(self):
        if self._win and self._win.winfo_exists():
            try:
                self._win.withdraw()
            except Exception:
                pass

    def _on_api_change(self):
        # called when APIs added/edited/deleted — refresh other tabs
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
            from src.icon_renderer import create_icon_image
            if self.app.icon:
                self.app.icon.icon = create_icon_image(self.app)
                if hasattr(self.app, "_rebuild_menu"):
                    self.app.icon.menu = self.app._rebuild_menu()
        except Exception:
            pass

    def _on_settings_saved(self):
        # called from SettingsFrame after successful save — refresh tray and history
        try:
            from src.icon_renderer import create_icon_image
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

    def refresh_all(self):
        for w in self._tabs.values():
            try:
                if hasattr(w, "refresh"):
                    w.refresh()
            except Exception:
                pass
