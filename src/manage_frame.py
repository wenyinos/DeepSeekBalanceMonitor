"""
Manage Frame — combined API management (top) + ledger table (bottom).
The ledger follows the API selected in the management table above.
"""
import tkinter as tk
from tkinter import ttk

from src.api_management_frame import ApiManagementFrame
from src.history_dialog import LedgerFrame


class ManageFrame(ttk.Frame):
    def __init__(self, parent, app, on_change=None):
        super().__init__(parent)
        self.app = app
        self._build(on_change)

    def _build(self, on_change):
        lang = self.app.lang
        # top: full API management (add/edit/delete table + forms)
        self.mgmt = ApiManagementFrame(self, self.app, on_change=on_change,
                                       on_select=self._on_mgmt_selected)
        self.mgmt.pack(fill="both", expand=True)

        ttk.Separator(self, orient="horizontal").pack(fill="x", padx=10, pady=(0, 2))

        # hint between separator and ledger table
        from src.config import T
        ttk.Label(self, text=T("ledger_hint", lang), font=("Segoe UI", 9, "bold"),
                  foreground="#555").pack(anchor="w", padx=10, pady=(0, 4))

        # bottom: ledger table + buttons, driven by selection above (no dropdown)
        self.ledger = LedgerFrame(self, self.app, show_selector=False)
        self.ledger.pack(fill="both", expand=True)

    def _on_mgmt_selected(self, api_id):
        """Management table selection drives the ledger below."""
        if api_id:
            self.ledger.set_api_id(api_id)

    # pass-throughs so MainWindow's generic on_show/refresh hooks keep working
    def on_show(self):
        self.mgmt.on_show()
        self.ledger.on_show()

    def refresh(self):
        try:
            self.mgmt.refresh()
        except Exception:
            pass
