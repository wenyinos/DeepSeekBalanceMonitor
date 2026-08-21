"""
History viewer — paginated balance records, trend chart, consumption rate, CSV export.
"""
import csv as _csv
import os
import sys as _sys
import tkinter as tk
from datetime import datetime, timedelta
from tkinter import filedialog, messagebox, ttk

from src.config import T, load_config, get_apis, get_api_by_id
from src.platforms import get_all_platforms as _get_plats, get_platform
from src.storage import export_all_csv, get_consumption_rate, get_history_by_date, get_history_page, get_package_history_page

_PLAT_META = _get_plats()

STATUS_SHORT = {
    "none": "OK", "minor": "Min", "major": "Maj",
    "critical": "Crit", "maintenance": "Mnt",
}


def open_history(app):
    """Open the history viewer window. Re-focuses if already open."""
    if app._history_open:
        try:
            app._history_window.deiconify()
            app._history_window.lift()
            app._history_window.after(50, app._history_window.focus_force)
        except Exception:
            pass
        return

    lang = app.lang

    # tk root is already initialised on the main thread in main()
    root = app._tk_root
    win = tk.Toplevel(root)
    app._history_open = True
    app._history_window = win

    def _cleanup():
        app._history_open = False
        app._history_window = None
        win.destroy()

    win.protocol("WM_DELETE_WINDOW", _cleanup)
    win.title(T("history", lang))
    win.geometry("850x640")
    win.minsize(500, 400)
    win.after(50, win.focus_force)
    win.update_idletasks()
    sw, sh = win.winfo_screenwidth(), win.winfo_screenheight()
    w, h = win.winfo_width(), win.winfo_height()
    win.geometry(f"+{(sw - w) // 2}+{(sh - h) // 2}")

    # App icon
    try:
        if getattr(_sys, "frozen", False):
            icon_path = os.path.join(_sys._MEIPASS, "app.ico")
        else:
            icon_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                     "assets", "app.ico")
        if os.path.isfile(icon_path):
            win.iconbitmap(icon_path)
    except Exception:
        pass

    # --- Treeview ----------------------------------------------------
    tree_frame = tk.Frame(win)
    tree_frame.pack(fill="both", expand=True, padx=10, pady=(10, 0))

    style = ttk.Style()
    style.configure("History.Treeview", rowheight=34, font=("Segoe UI", 9))

    tree = ttk.Treeview(tree_frame, columns=("time", "curr", "total", "topped", "granted", "status"),
                        show="headings", style="History.Treeview")
    tree.heading("time",   text=T("th_time", lang))
    tree.heading("curr",   text=T("th_currency", lang))
    tree.heading("total",  text=T("th_total", lang))
    tree.heading("topped", text=T("th_topped", lang))
    tree.heading("granted",text=T("th_granted", lang))
    tree.heading("status", text=T("th_status", lang))
    tree.column("time", width=220, minwidth=180)
    tree.column("curr", width=60, anchor="center", minwidth=50)
    tree.column("total", width=100, anchor="e", minwidth=80)
    tree.column("topped", width=100, anchor="e", minwidth=80)
    tree.column("granted", width=100, anchor="e", minwidth=80)
    tree.column("status", width=90, anchor="center", minwidth=75)

    scrollbar = tk.Scrollbar(tree_frame, orient="vertical", command=tree.yview)
    tree.configure(yscrollcommand=scrollbar.set)
    tree.pack(side="left", fill="both", expand=True)
    scrollbar.pack(side="right", fill="y")

    def _on_tree_wheel(event):
        tree.yview_scroll(int(-1 * (event.delta / 60)), "units")
    tree.bind("<MouseWheel>", _on_tree_wheel)
    tree.bind("<Enter>", lambda e: tree.bind_all("<MouseWheel>", _on_tree_wheel))
    tree.bind("<Leave>", lambda e: tree.unbind_all("<MouseWheel>"))

    # --- Chart -------------------------------------------------------
    chart_h = 150
    chart = tk.Canvas(win, height=chart_h, bg="#f5f5f5", highlightthickness=0)
    chart.pack(fill="x", padx=10, pady=(6, 0))

    # --- Rate label --------------------------------------------------
    rate_var = tk.StringVar()
    rate_label = tk.Label(win, textvariable=rate_var, font=("Segoe UI", 9),
                          fg="#555", anchor="w")
    rate_label.pack(fill="x", padx=14, pady=(2, 0))

    def _update_rate_label():
        if app.demo_mode:
            d = int(app._demo_hrs // 24)
            h = int(app._demo_hrs % 24)
            if d > 0:
                remaining = T("remaining_dh", lang, d=d, h=h)
            elif h >= 1:
                remaining = T("remaining_h", lang, h=h)
            else:
                remaining = T("remaining_lt1h", lang)
            prefix = T("est_prefix", lang)
            rate_var.set(T("rate_line", lang, rate=app._demo_rate, prefix=prefix, remaining=remaining))
            return
        cr = get_consumption_rate()
        if cr:
            hourly_rate, busy_hours, curr = cr
            days = int(busy_hours // 24)
            hrs = int(busy_hours % 24)
            if days > 0:
                remaining = T("remaining_dh", lang, d=days, h=hrs)
            elif hrs >= 1:
                remaining = T("remaining_h", lang, h=hrs)
            else:
                remaining = T("remaining_lt1h", lang)
            prefix = T("est_prefix", lang)
            rate_var.set(T("rate_line", lang, rate=hourly_rate, prefix=prefix, remaining=remaining))
        else:
            rate_var.set(T("not_enough_data", lang))

    # --- Data loading -------------------------------------------------
    offset_var = [0]
    all_rows = []
    btn_frame = ttk.Frame(win)
    btn_frame.pack(fill="x", side="bottom", padx=10, pady=10)
    load_btn = ttk.Button(btn_frame, text=T("load_more", lang))

    def _redraw_chart():
        # Reverse so oldest is on the left
        totals = [(r["total"], r["currency"]) for r in reversed(all_rows) if r["currency"]]
        totals = totals[-1000:]
        if len(totals) < 2:
            chart.delete("all")
            return
        chart.delete("all")
        cw = chart.winfo_width()
        ml, mr, mt, mb = 50, 12, 16, 28
        w = cw - ml - mr
        h = chart_h - mt - mb
        vals = [t[0] for t in totals]
        lo, hi = min(vals), max(vals)
        if hi == lo:
            hi = lo + 1

        chart.create_line(ml, mt, ml, mt + h, fill="#999", width=1)
        chart.create_line(ml, mt + h, ml + w, mt + h, fill="#999", width=1)

        for pct in (0, 0.5, 1):
            v = lo + (hi - lo) * pct
            y = mt + h * (1 - pct)
            chart.create_text(ml - 6, y, text=f"{v:.1f}", anchor="e",
                              fill="#666", font=("Segoe UI", 7))

        if all_rows:
            last_ts = all_rows[0]["timestamp"]
            n = min(len(all_rows), 1000)
            first_ts = all_rows[n - 1]["timestamp"]
        else:
            first_ts = last_ts = ""
        chart.create_text(ml, mt + h + 6, text=first_ts[:10] if len(first_ts) > 10 else first_ts,
                          anchor="nw", fill="#666", font=("Segoe UI", 7))
        chart.create_text(ml + w, mt + h + 6, text=last_ts[:10] if len(last_ts) > 10 else last_ts,
                          anchor="ne", fill="#666", font=("Segoe UI", 7))

        pts = []
        for i, v in enumerate(vals):
            x = ml + w * i / (len(vals) - 1)
            y = mt + h * (1 - (v - lo) / (hi - lo))
            pts.extend((x, y))
        if len(pts) >= 4:
            chart.create_line(pts, fill="#3C6966", width=2, smooth=True)
            for x, y in zip(pts[::2], pts[1::2]):
                chart.create_oval(x - 2, y - 2, x + 2, y + 2,
                                  fill="#3C6966", outline="")
        chart.configure(scrollregion=(0, 0, cw, chart_h))

    chart.bind("<Configure>", lambda e: _redraw_chart())

    def _load_page():
        if app.demo_mode:
            rows = app._demo_history[offset_var[0]:offset_var[0] + 100]
        else:
            rows = get_history_page(limit=100, offset=offset_var[0])
        for r in rows:
            s = r["service_status"]
            s_label = STATUS_SHORT.get(s, s) if s else "-"
            tree.insert("", "end", values=(
                r["timestamp"], r["currency"], f"{r['total']:.2f}",
                f"{r['topped']:.2f}", f"{r['granted']:.2f}", s_label,
            ))
        all_rows.extend(rows)
        offset_var[0] += len(rows)
        if len(rows) < 100:
            load_btn.configure(state="disabled",
                               text=T("all_loaded", lang))
        else:
            load_btn.configure(state="normal",
                               text=T("load_more", lang))
        _redraw_chart()
        _update_rate_label()

    def _export_csv():
        path = app.config.get("export_path", "").strip()
        if path:
            ts = datetime.now().strftime("%Y%m%d_%H%M%S")
            f = os.path.join(path, f"deepseek_balance_{ts}.csv")
        else:
            f = filedialog.asksaveasfilename(
                parent=win, defaultextension=".csv",
                filetypes=[("CSV files", "*.csv")],
                initialfile="deepseek_balance_history.csv",
            )
        if f:
            if app.demo_mode:
                with open(f, "w", newline="", encoding="utf-8-sig") as fh:
                    w = _csv.writer(fh)
                    w.writerow(["timestamp", "currency", "total", "topped", "granted", "service_status"])
                    for r in app._demo_history:
                        w.writerow([r["timestamp"], r["currency"], r["total"], r["topped"], r["granted"], r["service_status"]])
                n = len(app._demo_history)
            else:
                n = export_all_csv(f)
            msg = T("export_msg", lang, n=n)
            messagebox.showinfo("Export", msg, parent=win)

    export_btn = ttk.Button(btn_frame, text=T("export_csv_btn", lang),
                            command=_export_csv)

    load_btn.configure(command=_load_page)

    # --- Date filter -------------------------------------------------
    PLACEHOLDER = "YYYYMMDD"
    date_var = tk.StringVar(value=PLACEHOLDER)
    date_entry = ttk.Entry(btn_frame, textvariable=date_var, width=10)

    def _on_date_focus(e):
        if date_var.get() == PLACEHOLDER:
            date_var.set("")
            date_entry.configure(foreground="black")
    def _on_date_blur(e):
        if date_var.get() == "":
            date_var.set(PLACEHOLDER)
            date_entry.configure(foreground="gray")
    date_entry.configure(foreground="gray")
    date_entry.bind("<FocusIn>", _on_date_focus)
    date_entry.bind("<FocusOut>", _on_date_blur)

    def _query_by_date():
        d = date_var.get().strip()
        if d in ("", PLACEHOLDER):
            return
        if len(d) == 8 and d.isdigit():
            d = f"{d[:4]}-{d[4:6]}-{d[6:8]}"
        tree.delete(*tree.get_children())
        if app.demo_mode:
            rows = [r for r in app._demo_history if r["timestamp"].startswith(d)]
            all_rows.clear()
            all_rows.extend(rows)
        else:
            rows = get_history_by_date(d)
            all_rows.clear()
            all_rows.extend(reversed(rows))
        for r in rows:
            s = r["service_status"]
            s_label = STATUS_SHORT.get(s, s) if s else "-"
            tree.insert("", "end", values=(
                r["timestamp"], r["currency"], f"{r['total']:.2f}",
                f"{r['topped']:.2f}", f"{r['granted']:.2f}", s_label,
            ))
        reset_btn.configure(state="normal")
        _redraw_chart()
        _update_rate_label()
        load_btn.configure(state="disabled", text=T("all_loaded", lang))

    def _reset_query():
        date_var.set(PLACEHOLDER)
        date_entry.configure(foreground="gray")
        reset_btn.configure(state="disabled")
        tree.delete(*tree.get_children())
        offset_var[0] = 0
        all_rows.clear()
        _load_page()

    # --- Bottom bar layout -------------------------------------------
    load_btn.pack(side="left")
    export_btn.pack(side="left", padx=(6, 0))
    ttk.Separator(btn_frame, orient="vertical").pack(side="left", padx=8, fill="y")
    date_entry.pack(side="left", padx=(8, 4))
    query_btn = ttk.Button(btn_frame, text=T("filter_btn", lang), width=6, command=_query_by_date)
    reset_btn = ttk.Button(btn_frame, text=T("cancel_btn", lang), width=6, command=_reset_query)
    query_btn.pack(side="left")
    reset_btn.pack(side="left", padx=(4, 0))
    reset_btn.configure(state="disabled")

    _load_page()
    win.focus_force()


class HistoryFrame(ttk.Frame):
    """Embeddable history viewer for MainWindow Notebook."""
    def __init__(self, parent, app):
        super().__init__(parent)
        self.app = app
        self.lang = app.lang
        self._offset = [0]
        self._rows = []
        self._build()

    def _build(self):
        lang = self.lang
        # API selector (second-level tab for multi-API)
        api_bar = ttk.Frame(self)
        api_bar.pack(fill="x", padx=10, pady=(6, 0))
        ttk.Label(api_bar, text=T("select_api", lang)).pack(side="left")
        self.api_var = tk.StringVar()
        self.api_combo = ttk.Combobox(api_bar, textvariable=self.api_var, state="readonly", width=22)
        self.api_combo.pack(side="left", padx=(6, 0))
        self.api_combo.bind("<<ComboboxSelected>>", lambda e: self._on_api_selected())
        self._api_id_map = {}
        self._refresh_api_selector()
        # Tree — columns depend on API mode, rebuilt on API switch
        tree_frame = tk.Frame(self)
        tree_frame.pack(fill="both", expand=True, padx=10, pady=(10, 0))
        style = ttk.Style()
        style.configure("History.Treeview", rowheight=34, font=("Segoe UI", 9))
        self.tree = None  # created in _rebuild_tree
        self._current_mode = "payg"
        sb = tk.Scrollbar(tree_frame, orient="vertical")
        self.tree_frame = tree_frame

        chart_h = 150
        self.chart = tk.Canvas(self, height=chart_h, bg="#f5f5f5", highlightthickness=0)
        self.chart.pack(fill="x", padx=10, pady=(6, 0))
        self.chart_h = chart_h

        self.rate_var = tk.StringVar()
        tk.Label(self, textvariable=self.rate_var, font=("Segoe UI", 9), fg="#555", anchor="w").pack(fill="x", padx=14, pady=(2, 0))

        btn_frame = ttk.Frame(self)
        btn_frame.pack(fill="x", side="bottom", padx=10, pady=10)
        self.load_btn = ttk.Button(btn_frame, text=T("load_more", lang), command=self._load_page)
        self.load_btn.pack(side="left")
        self.export_btn = ttk.Button(btn_frame, text=T("export_csv_btn", lang), command=self._export_csv)
        self.export_btn.pack(side="left", padx=(6, 0))
        ttk.Separator(btn_frame, orient="vertical").pack(side="left", padx=8, fill="y")
        self.PLACEHOLDER = "YYYYMMDD"
        self.date_var = tk.StringVar(value=self.PLACEHOLDER)
        self.date_entry = ttk.Entry(btn_frame, textvariable=self.date_var, width=10)
        self.date_entry.configure(foreground="gray")
        self.date_entry.bind("<FocusIn>", self._on_date_focus)
        self.date_entry.bind("<FocusOut>", self._on_date_blur)
        self.date_entry.pack(side="left", padx=(8, 4))
        self.query_btn = ttk.Button(btn_frame, text=T("filter_btn", lang), width=6, command=self._query_by_date)
        self.query_btn.pack(side="left")
        self.reset_btn = ttk.Button(btn_frame, text=T("cancel_btn", lang), width=6, command=self._reset_query)
        self.reset_btn.pack(side="left", padx=(4, 0))
        self.reset_btn.configure(state="disabled")

        self.chart.bind("<Configure>", lambda e: self._redraw_chart())
        self._on_api_selected()  # sets correct tree columns and loads data

    def _on_date_focus(self, e):
        if self.date_var.get() == self.PLACEHOLDER:
            self.date_var.set("")
            self.date_entry.configure(foreground="black")
    def _on_date_blur(self, e):
        if self.date_var.get() == "":
            self.date_var.set(self.PLACEHOLDER)
            self.date_entry.configure(foreground="gray")

    def _rebuild_tree(self, mode="payg", pkg_windows=None, has_status=False):
        """Recreate tree with columns matching the API mode."""
        lang = self.app.lang
        self._current_mode = mode
        # destroy old tree
        if self.tree is not None:
            self.tree.destroy()
            self.tree = None
        # remove old scrollbar
        for w in self.tree_frame.winfo_children():
            if isinstance(w, tk.Scrollbar):
                w.destroy()
        if mode == "package":
            windows = pkg_windows or ["5h", "weekly", "monthly"]
            window_labels = {"5h": T("col_5h", lang), "weekly": T("col_weekly", lang), "monthly": T("col_monthly", lang)}
            cols = ["time"] + windows
            if has_status:
                cols.append("status")
            headings = {"time": T("th_time", lang)}
            widths = {"time": 200}
            for w in windows:
                headings[w] = window_labels.get(w, w)
                widths[w] = 120
            if has_status:
                headings["status"] = T("th_status", lang)
                widths["status"] = 90
        else:
            cols = ("time", "curr", "total", "topped", "granted", "status")
            headings = {
                "time": T("th_time", lang),
                "curr": T("th_currency", lang),
                "total": T("th_total", lang),
                "topped": T("th_topped", lang),
                "granted": T("th_granted", lang),
                "status": T("th_status", lang),
            }
            widths = {"time": 220, "curr": 60, "total": 100, "topped": 100, "granted": 100, "status": 90}
        self.tree = ttk.Treeview(self.tree_frame, columns=tuple(cols), show="headings", style="History.Treeview")
        pkg_ws = set(pkg_windows) if pkg_windows else set()
        for c in cols:
            self.tree.heading(c, text=headings[c])
            anchor = "center" if c in pkg_ws or c == "status" else ("e" if c not in ("time",) else "w")
            self.tree.column(c, width=widths[c], minwidth=60, anchor=anchor)
        sb = tk.Scrollbar(self.tree_frame, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side="left", fill="both", expand=True)
        sb.pack(side="right", fill="y")
        self.tree.bind("<MouseWheel>", lambda e: self.tree.yview_scroll(int(-1 * (e.delta / 60)), "units"))
        self.tree.bind("<Enter>", lambda e: self.tree.bind_all("<MouseWheel>", lambda ev: self.tree.yview_scroll(int(-1 * (ev.delta / 60)), "units")))
        self.tree.bind("<Leave>", lambda e: self.tree.unbind_all("<MouseWheel>"))

    def _get_selected_api_id(self):
        name = self.api_var.get()
        return self._api_id_map.get(name, "")

    def _refresh_api_selector(self):
        # repopulate api combobox from config
        try:
            cfg = load_config()
            apis = get_apis(cfg)
            # build display -> id map
            self._api_id_map.clear()
            displays = []
            for api in apis:
                plat = api.get("platform", "")
                plat_disp = next((p.display_name for p in _PLAT_META if p.key == plat), plat)
                disp = f"{api.get('name')} ({plat_disp})"
                displays.append(disp)
                self._api_id_map[disp] = api.get("id", "")
            self.api_combo["values"] = displays
            # always follow config preferred_api_id
            if displays:
                pref = cfg.get("preferred_api_id", "")
                pref_disp = next((d for d, aid in self._api_id_map.items() if aid == pref), displays[0])
                self.api_var.set(pref_disp)
            else:
                self.api_var.set("")
                self._api_id_map.clear()
        except Exception:
            pass

    def _on_api_selected(self):
        # determine mode from selected API
        api_id = self._get_selected_api_id()
        mode = "payg"
        pkg_windows = None
        has_status = False
        if api_id:
            try:
                api = get_api_by_id(api_id)
                if api:
                    mode = api.get("mode", "payg")
                    pmeta = get_platform(api.get("platform", ""))
                    if pmeta:
                        pkg_windows = pmeta.package_windows
                        has_status = pmeta.has_status_page
            except Exception:
                pass
        self._rebuild_tree(mode, pkg_windows, has_status)
        # reload data for new API
        self._offset[0] = 0
        self._rows.clear()
        self.reset_btn.configure(state="disabled")
        self.date_var.set(self.PLACEHOLDER)
        self.date_entry.configure(foreground="gray")
        self._load_page()

    def refresh_api_selector(self):
        self._refresh_api_selector()
        # also reload data for new selection
        self._on_api_selected()

    def _update_rate(self):
        lang = self.app.lang
        if self.app.demo_mode:
            d = int(self.app._demo_hrs // 24); h = int(self.app._demo_hrs % 24)
            rem = T("remaining_dh", lang, d=d, h=h) if d>0 else (T("remaining_h", lang, h=h) if h>=1 else T("remaining_lt1h", lang))
            self.rate_var.set(T("rate_line", lang, rate=self.app._demo_rate, prefix=T("est_prefix", lang), remaining=rem))
            return
        api_id = self._get_selected_api_id()
        if self._current_mode == "package":
            # calculate package rate from package_history
            rate_str = self._calc_package_rate(api_id, lang)
            self.rate_var.set(rate_str)
        else:
            cr = get_consumption_rate(api_id=api_id) if api_id else get_consumption_rate()
            if cr:
                hr, bh, _ = cr
                total_hrs = round(bh, 1)
                self.rate_var.set(T("rate_line", lang, rate=hr, prefix=T("est_prefix", lang), remaining=f"{total_hrs}"))
            else:
                self.rate_var.set(T("not_enough_data", lang))

    def _calc_package_rate(self, api_id, lang):
        """Calculate hourly rate of quota consumption from package_history.
        Uses the API's billing_period setting."""
        try:
            rows = get_package_history_page(limit=100, api_id=api_id or None)
            if len(rows) < 2:
                return T("not_enough_data", lang)
            # get billing period from API config
            billing_period = "monthly"
            try:
                api = get_api_by_id(api_id) if api_id else None
                if api:
                    billing_period = api.get("billing_period") or "monthly"
            except Exception:
                pass
            period_map = {"5h": "h5_percent", "weekly": "weekly_percent", "monthly": "monthly_percent"}
            period_labels = {"5h": T("unit_5h", lang), "weekly": T("unit_weekly", lang), "monthly": T("unit_monthly", lang)}
            col = period_map.get(billing_period, "monthly_percent")
            newest = rows[0]
            oldest = rows[-1]
            newest_pct = newest.get(col) or 0
            oldest_pct = oldest.get(col) or 0
            t_new = datetime.strptime(newest["timestamp"], "%Y-%m-%d %H:%M:%S")
            t_old = datetime.strptime(oldest["timestamp"], "%Y-%m-%d %H:%M:%S")
            hours = (t_new - t_old).total_seconds() / 3600
            if hours <= 0:
                return T("not_enough_data", lang)
            # used % per hour (positive = consuming)
            hourly_rate = (newest_pct - oldest_pct) / hours  # %/hr
            if hourly_rate <= 0:
                return T("not_enough_data", lang)
            remaining_pct = 100 - newest_pct
            remaining_hours = remaining_pct / hourly_rate
            total_hrs = round(remaining_hours, 1)
            unit = period_labels.get(billing_period, T("unit_monthly", lang))
            return T("pkg_rate_line", lang, rate=hourly_rate, unit=unit, remaining=total_hrs)
        except Exception as e:
            return T("not_enough_data", lang)

    def _redraw_chart(self):
        all_rows = self._rows
        chart = self.chart
        chart_h = self.chart_h
        if self._current_mode == "package":
            # package mode: plot remaining % using billing_period's column
            billing_period = "monthly"
            try:
                api_id = self._get_selected_api_id()
                api = get_api_by_id(api_id) if api_id else None
                if api:
                    billing_period = api.get("billing_period") or "monthly"
            except Exception:
                pass
            col_map = {"5h": "h5_percent", "weekly": "weekly_percent", "monthly": "monthly_percent"}
            col = col_map.get(billing_period, "monthly_percent")
            vals = [100 - (r.get(col) or 0) for r in reversed(all_rows)]
        else:
            vals = [r["total"] for r in reversed(all_rows) if r.get("currency")]
        vals = vals[-1000:]
        if len(vals) < 2:
            chart.delete("all"); return
        chart.delete("all")
        cw = chart.winfo_width()
        if cw < 50:
            chart.after(80, self._redraw_chart); return
        ml, mr, mt, mb = 50, 12, 16, 28
        w = cw - ml - mr
        h = chart_h - mt - mb
        lo, hi = min(vals), max(vals)
        if hi == lo: hi = lo + 1
        chart.create_line(ml, mt, ml, mt + h, fill="#999", width=1)
        chart.create_line(ml, mt + h, ml + w, mt + h, fill="#999", width=1)
        for pct in (0, 0.5, 1):
            v = lo + (hi - lo) * pct
            y = mt + h * (1 - pct)
            chart.create_text(ml - 6, y, text=f"{v:.1f}", anchor="e", fill="#666", font=("Segoe UI", 7))
        if all_rows:
            last_ts = all_rows[0]["timestamp"]; n = min(len(all_rows), 1000)
            first_ts = all_rows[n - 1]["timestamp"]
        else:
            first_ts = last_ts = ""
        chart.create_text(ml, mt + h + 6, text=first_ts[:10] if len(first_ts) > 10 else first_ts, anchor="nw", fill="#666", font=("Segoe UI", 7))
        chart.create_text(ml + w, mt + h + 6, text=last_ts[:10] if len(last_ts) > 10 else last_ts, anchor="ne", fill="#666", font=("Segoe UI", 7))
        pts = []
        for i, v in enumerate(vals):
            x = ml + w * i / (len(vals) - 1)
            y = mt + h * (1 - (v - lo) / (hi - lo))
            pts.extend((x, y))
        if len(pts) >= 4:
            chart.create_line(pts, fill="#3C6966", width=2, smooth=True)
            for x, y in zip(pts[::2], pts[1::2]):
                chart.create_oval(x - 2, y - 2, x + 2, y + 2, fill="#3C6966", outline="")

    def _load_page(self):
        api_id = self._get_selected_api_id()
        if self.app.demo_mode:
            rows = self.app._demo_history[self._offset[0]:self._offset[0] + 100]
        elif self._current_mode == "package":
            rows = get_package_history_page(limit=100, offset=self._offset[0], api_id=api_id or None)
        else:
            rows = get_history_page(limit=100, offset=self._offset[0], api_id=api_id or None)
        for r in rows:
            if self._current_mode == "package":
                vals = [r["timestamp"]]
                for c in self.tree["columns"]:
                    if c == "time":
                        continue
                    if c == "status":
                        ss = r.get("service_status")
                        if ss:
                            vals.append(STATUS_SHORT.get(ss, ss))
                        else:
                            vals.append("-")
                        continue
                    # map tree column to data key
                    if c == "5h":
                        pct = r.get("h5_percent")
                    elif c == "weekly":
                        pct = r.get("weekly_percent")
                    elif c == "monthly":
                        pct = r.get("monthly_percent")
                    else:
                        pct = None
                    vals.append(f"{100 - (pct or 0):.0f}%" if pct is not None else "—")
                self.tree.insert("", "end", values=tuple(vals))
            else:
                s_label = STATUS_SHORT.get(r["service_status"], r["service_status"]) if r["service_status"] else "-"
                self.tree.insert("", "end", values=(r["timestamp"], r["currency"], f"{r['total']:.2f}", f"{r['topped']:.2f}", f"{r['granted']:.2f}", s_label))
        self._rows.extend(rows)
        self._offset[0] += len(rows)
        if len(rows) < 100:
            self.load_btn.configure(state="disabled", text=T("all_loaded", self.app.lang))
        else:
            self.load_btn.configure(state="normal", text=T("load_more", self.app.lang))
        self._redraw_chart(); self._update_rate()

    def _export_csv(self):
        path = self.app.config.get("export_path", "").strip()
        parent = self.winfo_toplevel()
        if path:
            ts = datetime.now().strftime("%Y%m%d_%H%M%S")
            f = os.path.join(path, f"deepseek_balance_{ts}.csv")
        else:
            f = filedialog.asksaveasfilename(parent=parent, defaultextension=".csv", filetypes=[("CSV files", "*.csv")], initialfile="deepseek_balance_history.csv")
        if f:
            if self.app.demo_mode:
                with open(f, "w", newline="", encoding="utf-8-sig") as fh:
                    w = _csv.writer(fh)
                    w.writerow(["timestamp", "currency", "total", "topped", "granted", "service_status"])
                    for r in self.app._demo_history:
                        w.writerow([r["timestamp"], r["currency"], r["total"], r["topped"], r["granted"], r["service_status"]])
                n = len(self.app._demo_history)
            else:
                api_id = self._get_selected_api_id()
                if self._current_mode == "package":
                    rows = get_package_history_page(limit=9999, api_id=api_id or None)
                    with open(f, "w", newline="", encoding="utf-8-sig") as fh:
                        w = _csv.writer(fh)
                        w.writerow(["timestamp", T("col_5h", lang), T("col_weekly", lang), T("col_monthly", lang)])
                        for r in rows:
                            h5 = 100 - (r.get("h5_percent") or 0)
                            wk = 100 - (r.get("weekly_percent") or 0)
                            mo = 100 - (r.get("monthly_percent") or 0)
                            w.writerow([r["timestamp"], f"{h5:.0f}%", f"{wk:.0f}%", f"{mo:.0f}%"])
                    n = len(rows)
                else:
                    n = export_all_csv(f, api_id=api_id or None)
            messagebox.showinfo("Export", T("export_msg", self.app.lang, n=n), parent=parent)

    def _query_by_date(self):
        d = self.date_var.get().strip()
        if d in ("", self.PLACEHOLDER): return
        if len(d) == 8 and d.isdigit(): d = f"{d[:4]}-{d[4:6]}-{d[6:8]}"
        self.tree.delete(*self.tree.get_children())
        api_id = self._get_selected_api_id()
        if self.app.demo_mode:
            rows = [r for r in self.app._demo_history if r["timestamp"].startswith(d)]
            self._rows.clear(); self._rows.extend(rows)
        else:
            rows = get_history_by_date(d, api_id=api_id or None)
            self._rows.clear(); self._rows.extend(reversed(rows))
        for r in rows:
            s_label = STATUS_SHORT.get(r["service_status"], r["service_status"]) if r["service_status"] else "-"
            self.tree.insert("", "end", values=(r["timestamp"], r["currency"], f"{r['total']:.2f}", f"{r['topped']:.2f}", f"{r['granted']:.2f}", s_label))
        self.reset_btn.configure(state="normal")
        self._redraw_chart(); self._update_rate()
        self.load_btn.configure(state="disabled", text=T("all_loaded", self.app.lang))

    def _reset_query(self):
        self.date_var.set(self.PLACEHOLDER); self.date_entry.configure(foreground="gray")
        self.reset_btn.configure(state="disabled")
        self.tree.delete(*self.tree.get_children())
        self._offset[0] = 0; self._rows.clear()
        self._load_page()

    def _reload(self):
        # reload first page (used for new data); keep filter if active
        if self.date_var.get().strip() not in ("", self.PLACEHOLDER):
            return
        try:
            self.tree.delete(*self.tree.get_children())
        except Exception:
            pass
        self._offset[0] = 0
        self._rows.clear()
        self._load_page()

    def on_show(self):
        self._refresh_api_selector()
        self._on_api_selected()
    def refresh(self):
        self._update_rate()
        # if new records arrived while tab was hidden, reload
        try:
            if not self._rows:
                self._reload()
            else:
                api_id = self._get_selected_api_id()
                latest = get_history_page(limit=1, offset=0, api_id=api_id or None)
                if latest and latest[0]["timestamp"] != self._rows[0]["timestamp"]:
                    self._reload()
        except Exception:
            pass
        try:
            self.tree.heading("time", text=T("th_time", self.app.lang))
            self.tree.heading("curr", text=T("th_currency", self.app.lang))
            self.tree.heading("total", text=T("th_total", self.app.lang))
            self.tree.heading("topped", text=T("th_topped", self.app.lang))
            self.tree.heading("granted", text=T("th_granted", self.app.lang))
            self.tree.heading("status", text=T("th_status", self.app.lang))
        except Exception:
            pass
