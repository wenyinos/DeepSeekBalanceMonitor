"""
Balance history storage — SQLite-backed, for spend-rate / trend analysis.
"""
import csv
import sqlite3
from datetime import datetime

from src.config import DB_FILE, CONFIG_DIR, log


def _connect():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(DB_FILE))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS balance_history (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp       TEXT    NOT NULL,
            currency        TEXT    NOT NULL,
            total           REAL    NOT NULL,
            topped          REAL    NOT NULL,
            granted         REAL    NOT NULL,
            service_status  TEXT
        )
    """)
    # Migrate: add column if missing from older DB
    try:
        conn.execute("ALTER TABLE balance_history ADD COLUMN service_status TEXT")
    except sqlite3.OperationalError:
        pass
    conn.commit()
    return conn


def save_balance_record(currency: str, total: float, topped: float, granted: float,
                        service_status: str | None = None):
    """Insert one balance record. Called after each successful balance check."""
    try:
        conn = _connect()
        ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        conn.execute(
            "INSERT INTO balance_history (timestamp, currency, total, topped, granted, service_status) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (ts, currency, total, topped, granted, service_status),
        )
        conn.commit()
        conn.close()
    except Exception as e:
        log(f"Failed to save balance record: {e}")


def get_balance_history(days: int = 30):
    """Return the last N days of balance records as a list of dicts."""
    try:
        conn = _connect()
        cur = conn.execute(
            "SELECT timestamp, currency, total, topped, granted "
            "FROM balance_history "
            "WHERE timestamp >= datetime('now', ?) "
            "ORDER BY timestamp ASC",
            (f"-{days} days",),
        )
        rows = [
            {"timestamp": r[0], "currency": r[1], "total": r[2],
             "topped": r[3], "granted": r[4]}
            for r in cur.fetchall()
        ]
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read balance history: {e}")
        return []


def get_history_page(limit: int = 100, offset: int = 0):
    """Return one page of balance records, newest first."""
    try:
        conn = _connect()
        cur = conn.execute(
            "SELECT timestamp, currency, total, topped, granted, service_status "
            "FROM balance_history "
            "ORDER BY timestamp DESC "
            "LIMIT ? OFFSET ?",
            (limit, offset),
        )
        rows = [
            {"timestamp": r[0], "currency": r[1], "total": r[2],
             "topped": r[3], "granted": r[4], "service_status": r[5]}
            for r in cur.fetchall()
        ]
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read history page: {e}")
        return []


def get_history_by_date(date_str: str):
    """Return all balance records for a specific date (YYYY-MM-DD)."""
    try:
        conn = _connect()
        cur = conn.execute(
            "SELECT timestamp, currency, total, topped, granted, service_status "
            "FROM balance_history "
            "WHERE timestamp LIKE ? "
            "ORDER BY timestamp ASC",
            (f"{date_str}%",),
        )
        rows = [
            {"timestamp": r[0], "currency": r[1], "total": r[2],
             "topped": r[3], "granted": r[4], "service_status": r[5]}
            for r in cur.fetchall()
        ]
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read history by date: {e}")
        return []


def export_all_csv(path: str) -> int:
    """Export all balance records to a CSV file. Returns row count."""
    try:
        conn = _connect()
        cur = conn.execute(
            "SELECT timestamp, currency, total, topped, granted, service_status "
            "FROM balance_history ORDER BY timestamp ASC"
        )
        count = 0
        with open(path, "w", newline="", encoding="utf-8-sig") as f:
            w = csv.writer(f)
            w.writerow(["timestamp", "currency", "total", "topped", "granted", "service_status"])
            for r in cur:
                w.writerow(r)
                count += 1
        conn.close()
        return count
    except Exception as e:
        log(f"Failed to export CSV: {e}")
        return 0


def get_consumption_rate(days=7):
    """Busy-hour weighted hourly consumption rate from topped-up balance.
    Splits on top-ups, long idle gaps (>m), and long flat periods (>m).
    Returns (hourly_rate, busy_hours_remaining, currency) or None.
    Falls back to the full retention window if default is insufficient."""
    result = _get_consumption_rate_for_days(days)
    if result or days != 7:
        return result
    try:
        from src.config import load_config
        retention_days = int(load_config().get("retention_days", 30))
    except Exception:
        retention_days = 30
    fallback_days = max(days, retention_days)
    if fallback_days <= days:
        return result
    return _get_consumption_rate_for_days(fallback_days)


def _get_consumption_rate_for_days(days=7):
    """Busy-hour slicing: split on top-ups, long idle gaps, and long flat periods.
    Only "busy" intervals (frequent polling with actual consumption) contribute
    to the weighted hourly rate.  Returns (hourly_rate, busy_hours_remaining,
    currency) or None."""
    try:
        conn = _connect()
        cur = conn.execute(
            "SELECT timestamp, currency, topped "
            "FROM balance_history "
            "WHERE timestamp >= datetime('now', ?) "
            "ORDER BY timestamp ASC",
            (f"-{days} days",),
        )
        rows = cur.fetchall()
        conn.close()
        if len(rows) < 2:
            return None

        # Parse records and determine the busy threshold m
        parsed = []
        for r in rows:
            parsed.append((datetime.strptime(r[0], "%Y-%m-%d %H:%M:%S"),
                           r[1], r[2]))  # (ts, currency, topped)

        currency = parsed[0][1]
        try:
            from src.config import load_config
            interval_min = int(load_config().get("interval_minutes", 10))
        except Exception:
            interval_min = 10
        m_minutes = max(30, 2 * interval_min)
        m_sec = m_minutes * 60

        # --- Build busy intervals ----------------------------------------
        intervals = []  # (start_val, start_ts, end_val, end_ts)
        seg_start_val = parsed[0][2]
        seg_start_ts = parsed[0][0]
        prev_val = seg_start_val
        prev_ts = seg_start_ts
        eq_start_idx = None  # index in parsed where current equal run began

        for i in range(1, len(parsed)):
            curr_ts, curr_curr, curr_val = parsed[i]
            gap_sec = (curr_ts - prev_ts).total_seconds()

            if curr_val > prev_val:
                # Rule 1 — top-up: close previous interval, start fresh
                if eq_start_idx is not None:
                    eq_dur = (prev_ts - parsed[eq_start_idx][0]).total_seconds()
                    if eq_dur > m_sec:
                        if parsed[eq_start_idx][0] > seg_start_ts:
                            intervals.append((seg_start_val, seg_start_ts,
                                             parsed[eq_start_idx][2],
                                             parsed[eq_start_idx][0]))
                        seg_start_val = curr_val
                        seg_start_ts = curr_ts
                        prev_val = curr_val
                        prev_ts = curr_ts
                        eq_start_idx = None
                        continue
                    eq_start_idx = None

                if prev_ts > seg_start_ts:
                    intervals.append((seg_start_val, seg_start_ts,
                                     prev_val, prev_ts))
                seg_start_val = curr_val
                seg_start_ts = curr_ts

            elif curr_val < prev_val:
                if gap_sec > m_sec:
                    # Rule 2 — long idle gap: slice.
                    # Process any pending equal run FIRST so long flat
                    # periods aren't smuggled into the interval.
                    if eq_start_idx is not None:
                        eq_dur = (prev_ts - parsed[eq_start_idx][0]).total_seconds()
                        if eq_dur > m_sec:
                            if parsed[eq_start_idx][0] > seg_start_ts:
                                intervals.append((seg_start_val, seg_start_ts,
                                                 parsed[eq_start_idx][2],
                                                 parsed[eq_start_idx][0]))
                            seg_start_val = prev_val
                            seg_start_ts = prev_ts
                        eq_start_idx = None
                    if prev_ts > seg_start_ts:
                        intervals.append((seg_start_val, seg_start_ts,
                                         prev_val, prev_ts))
                    seg_start_val = curr_val
                    seg_start_ts = curr_ts
                else:
                    # Normal consumption — may follow a short equal run
                    if eq_start_idx is not None:
                        eq_dur = (prev_ts - parsed[eq_start_idx][0]).total_seconds()
                        if eq_dur > m_sec:
                            # Rule 3 — long flat: discard it
                            if parsed[eq_start_idx][0] > seg_start_ts:
                                intervals.append((seg_start_val, seg_start_ts,
                                                 parsed[eq_start_idx][2],
                                                 parsed[eq_start_idx][0]))
                            seg_start_val = curr_val
                            seg_start_ts = curr_ts
                            prev_val = curr_val
                            prev_ts = curr_ts
                            eq_start_idx = None
                            continue
                        eq_start_idx = None
            else:
                # curr_val == prev_val — Rule 3: track equal run
                if eq_start_idx is None:
                    eq_start_idx = i - 1

            prev_val = curr_val
            prev_ts = curr_ts

        # Handle trailing equal run (never resolved because value didn't change)
        if eq_start_idx is not None:
            eq_dur = (parsed[-1][0] - parsed[eq_start_idx][0]).total_seconds()
            if eq_dur > m_sec:
                if parsed[eq_start_idx][0] > seg_start_ts:
                    intervals.append((seg_start_val, seg_start_ts,
                                     parsed[eq_start_idx][2],
                                     parsed[eq_start_idx][0]))
                seg_start_ts = parsed[-1][0]  # prevent double-add below

        # Close final interval
        if parsed[-1][0] > seg_start_ts:
            intervals.append((seg_start_val, seg_start_ts,
                             prev_val, parsed[-1][0]))

        # --- Weighted average of hourly rates ----------------------------
        total_weight = 0.0
        weighted_sum = 0.0
        for sv, st, ev, et in intervals:
            if ev >= sv:
                continue
            delta_h = (et - st).total_seconds() / 3600
            if delta_h < 0.01:
                continue
            hourly_rate = (sv - ev) / delta_h
            weighted_sum += hourly_rate * delta_h
            total_weight += delta_h

        if total_weight == 0:
            return None

        avg_hourly = weighted_sum / total_weight
        if avg_hourly <= 0:
            return None

        remaining = parsed[-1][2]
        busy_hours = remaining / avg_hourly
        return avg_hourly, busy_hours, currency
    except Exception as e:
        log(f"Failed to compute consumption rate: {e}")
        return None


def prune_old_data(retention_days: int):
    """Delete balance records and log entries older than retention_days.
    Called once on startup."""
    try:
        conn = _connect()
        conn.execute(
            "DELETE FROM balance_history "
            "WHERE timestamp < datetime('now', ?)",
            (f"-{retention_days} days",),
        )
        conn.commit()
        conn.execute("VACUUM")
        conn.close()
        log(f"Pruned balance history older than {retention_days} days")
    except Exception as e:
        log(f"Failed to prune balance history: {e}")

    try:
        from src.config import LOG_FILE
        if not LOG_FILE.exists():
            return
        cutoff = datetime.now().timestamp() - retention_days * 86400
        lines = LOG_FILE.read_text(encoding="utf-8").splitlines()
        kept = []
        for line in lines:
            try:
                ts_str = line[1:20]  # "[YYYY-MM-DD HH:MM:SS]"
                ts = datetime.strptime(ts_str, "%Y-%m-%d %H:%M:%S").timestamp()
                if ts >= cutoff:
                    kept.append(line)
            except (ValueError, IndexError):
                kept.append(line)
        LOG_FILE.write_text("\n".join(kept) + "\n", encoding="utf-8")
        log(f"Pruned log entries older than {retention_days} days")
    except Exception as e:
        log(f"Failed to prune log file: {e}")
