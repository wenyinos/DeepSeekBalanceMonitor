"""
History viewer — paginated balance records, trend chart, consumption rate, CSV export.
"""
import csv as _csv
import os
import sys as _sys
import tkinter as tk
from datetime import datetime, timedelta
from tkinter import filedialog, messagebox, ttk

from src.config import T
from src.storage import export_all_csv, get_consumption_rate, get_history_by_date, get_history_page

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

    if app._tk_root is None:
        app._tk_root = tk.Tk()
        app._tk_root.withdraw()
    root = app._tk_root
    win = tk.Toplevel(root)
    app._history_open = True
    app._history_window = win

    def _cleanup():
        app._history_open = False
        app._history_window = None
        win.destroy()
        root.quit()

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
            rate_var.set(T("rate_line", lang, rate=app._demo_daily, prefix=prefix, remaining=remaining))
            return
        cr = get_consumption_rate()
        if cr:
            daily_rate, hours_left, curr = cr
            days = int(hours_left // 24)
            hrs = int(hours_left % 24)
            if days > 0:
                remaining = T("remaining_dh", lang, d=days, h=hrs)
            elif hrs >= 1:
                remaining = T("remaining_h", lang, h=hrs)
            else:
                remaining = T("remaining_lt1h", lang)
            prefix = T("est_prefix", lang)
            rate_var.set(T("rate_line", lang, rate=daily_rate, prefix=prefix, remaining=remaining))
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
    root.mainloop()
