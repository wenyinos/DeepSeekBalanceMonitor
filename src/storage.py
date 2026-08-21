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
            service_status  TEXT,
            api_id          TEXT
        )
    """)
    # Migrate: add columns if missing from older DB
    for col in ("service_status TEXT", "api_id TEXT"):
        try:
            conn.execute(f"ALTER TABLE balance_history ADD COLUMN {col}")
        except sqlite3.OperationalError:
            pass
    # Migrate legacy rows with NULL api_id to preferred_api_id if available
    try:
        cur = conn.execute("SELECT COUNT(*) FROM balance_history WHERE api_id IS NULL OR api_id=''")
        cnt = cur.fetchone()[0]
        if cnt > 0:
            from src.config import load_config
            cfg = load_config()
            pref = cfg.get("preferred_api_id") or (cfg.get("apis") or [{}])[0].get("id") if cfg.get("apis") else None
            if pref:
                conn.execute("UPDATE balance_history SET api_id=? WHERE api_id IS NULL OR api_id=''", (pref,))
                conn.commit()
    except Exception:
        pass
    conn.commit()
    return conn


def save_balance_record(currency: str, total: float, topped: float, granted: float,
                        service_status: str | None = None, api_id: str | None = None):
    """Insert one balance record. Called after each successful balance check."""
    try:
        conn = _connect()
        ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        # resolve api_id if not given
        if not api_id:
            try:
                from src.config import load_config
                api_id = load_config().get("preferred_api_id", "")
            except Exception:
                api_id = ""
        conn.execute(
            "INSERT INTO balance_history (timestamp, currency, total, topped, granted, service_status, api_id) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (ts, currency, total, topped, granted, service_status, api_id or ""),
        )
        conn.commit()
        conn.close()
    except Exception as e:
        log(f"Failed to save balance record: {e}")


def get_history_page(limit: int = 100, offset: int = 0, api_id: str | None = None):
    """Return one page of balance records, newest first. Filter by api_id if given."""
    try:
        conn = _connect()
        if api_id:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id "
                "FROM balance_history WHERE api_id=? "
                "ORDER BY timestamp DESC LIMIT ? OFFSET ?",
                (api_id, limit, offset),
            )
        else:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id "
                "FROM balance_history "
                "ORDER BY timestamp DESC LIMIT ? OFFSET ?",
                (limit, offset),
            )
        rows = [
            {"timestamp": r[0], "currency": r[1], "total": r[2],
             "topped": r[3], "granted": r[4], "service_status": r[5], "api_id": r[6] if len(r) > 6 else ""}
            for r in cur.fetchall()
        ]
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read history page: {e}")
        return []


def get_history_by_date(date_str: str, api_id: str | None = None):
    """Return all balance records for a specific date (YYYY-MM-DD). Filter by api_id if given."""
    try:
        conn = _connect()
        if api_id:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id "
                "FROM balance_history WHERE timestamp LIKE ? AND api_id=? ORDER BY timestamp ASC",
                (f"{date_str}%", api_id),
            )
        else:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id "
                "FROM balance_history WHERE timestamp LIKE ? ORDER BY timestamp ASC",
                (f"{date_str}%",),
            )
        rows = [
            {"timestamp": r[0], "currency": r[1], "total": r[2],
             "topped": r[3], "granted": r[4], "service_status": r[5], "api_id": r[6] if len(r) > 6 else ""}
            for r in cur.fetchall()
        ]
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read history by date: {e}")
        return []


def export_all_csv(path: str, api_id: str | None = None) -> int:
    """Export balance records to CSV. Filter by api_id if given."""
    try:
        conn = _connect()
        if api_id:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id FROM balance_history WHERE api_id=? ORDER BY timestamp ASC", (api_id,))
        else:
            cur = conn.execute(
                "SELECT timestamp, currency, total, topped, granted, service_status, api_id FROM balance_history ORDER BY timestamp ASC"
            )
        count = 0
        with open(path, "w", newline="", encoding="utf-8-sig") as f:
            w = csv.writer(f)
            w.writerow(["timestamp", "currency", "total", "topped", "granted", "service_status", "api_id"])
            for r in cur:
                w.writerow(r)
                count += 1
        conn.close()
        return count
    except Exception as e:
        log(f"Failed to export CSV: {e}")
        return 0


def get_consumption_rate(days=7, api_id: str | None = None):
    """Busy-hour weighted hourly consumption rate. Filter by api_id if given."""
    result = _get_consumption_rate_for_days(days, _interval_min=None, api_id=api_id)
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
    return _get_consumption_rate_for_days(fallback_days, _interval_min=None, api_id=api_id)


def _get_consumption_rate_for_days(days=7, _interval_min=None, api_id: str | None = None):
    """Busy-hour slicing: split on top-ups, long idle gaps, and long flat periods.
    Only "busy" intervals contribute to the weighted hourly rate. Filter by api_id if given."""
    try:
        conn = _connect()
        if api_id:
            cur = conn.execute(
                "SELECT timestamp, currency, topped FROM balance_history WHERE timestamp >= datetime('now', ?) AND api_id=? ORDER BY timestamp ASC",
                (f"-{days} days", api_id),
            )
        else:
            cur = conn.execute(
                "SELECT timestamp, currency, topped FROM balance_history WHERE timestamp >= datetime('now', ?) ORDER BY timestamp ASC",
                (f"-{days} days",),
            )
        rows = cur.fetchall()
        conn.close()
        if len(rows) < 2:
            return None

        parsed = [(datetime.strptime(r[0], "%Y-%m-%d %H:%M:%S"), r[1], r[2]) for r in rows]
        currency = parsed[0][1]
        if _interval_min is None:
            try:
                from src.config import load_config
                _interval_min = int(load_config().get("interval_minutes", 10))
            except Exception:
                _interval_min = 10
        m_sec = max(30, 2 * _interval_min) * 60

        intervals = _slice_busy_intervals(parsed, m_sec)

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
        busy_hours = parsed[-1][2] / avg_hourly
        return avg_hourly, busy_hours, currency
    except Exception as e:
        log(f"Failed to compute consumption rate: {e}")
        return None


def _slice_busy_intervals(parsed, m_sec):
    """Pure function: split parsed [(ts,curr,val)] into busy intervals."""
    intervals = []
    seg_start_val = parsed[0][2]
    seg_start_ts = parsed[0][0]
    prev_val = seg_start_val
    prev_ts = seg_start_ts
    eq_start_idx = None

    def _flush_eq_as_interval(end_idx):
        """If the equal run ending at end_idx is long, flush segment before it."""
        nonlocal seg_start_val, seg_start_ts
        eq_dur = (parsed[end_idx][0] - parsed[eq_start_idx][0]).total_seconds()
        if eq_dur > m_sec and parsed[eq_start_idx][0] > seg_start_ts:
            intervals.append((seg_start_val, seg_start_ts,
                              parsed[eq_start_idx][2], parsed[eq_start_idx][0]))
            return True
        return False

    for i in range(1, len(parsed)):
        curr_ts, _, curr_val = parsed[i]
        gap_sec = (curr_ts - prev_ts).total_seconds()

        if curr_val > prev_val:  # Rule 1: top-up
            if eq_start_idx is not None:
                if _flush_eq_as_interval(i - 1):
                    seg_start_val = curr_val
                    seg_start_ts = curr_ts
                    prev_val = curr_val
                    prev_ts = curr_ts
                    eq_start_idx = None
                    continue
                eq_start_idx = None
            if prev_ts > seg_start_ts:
                intervals.append((seg_start_val, seg_start_ts, prev_val, prev_ts))
            seg_start_val = curr_val
            seg_start_ts = curr_ts

        elif curr_val < prev_val:  # consumption drop
            if gap_sec > m_sec:  # Rule 2: long idle gap
                if eq_start_idx is not None:
                    if _flush_eq_as_interval(i - 1):
                        seg_start_val = prev_val
                        seg_start_ts = prev_ts
                    eq_start_idx = None
                if prev_ts > seg_start_ts:
                    intervals.append((seg_start_val, seg_start_ts, prev_val, prev_ts))
                seg_start_val = curr_val
                seg_start_ts = curr_ts
            else:  # normal consumption, may follow short equal run
                if eq_start_idx is not None:
                    eq_dur = (prev_ts - parsed[eq_start_idx][0]).total_seconds()
                    if eq_dur > m_sec:  # Rule 3: long flat discard
                        if parsed[eq_start_idx][0] > seg_start_ts:
                            intervals.append((seg_start_val, seg_start_ts,
                                              parsed[eq_start_idx][2], parsed[eq_start_idx][0]))
                        seg_start_val = curr_val
                        seg_start_ts = curr_ts
                        prev_val = curr_val
                        prev_ts = curr_ts
                        eq_start_idx = None
                        continue
                    eq_start_idx = None
        else:  # curr_val == prev_val — Rule 3: track equal run
            if eq_start_idx is None:
                eq_start_idx = i - 1

        prev_val = curr_val
        prev_ts = curr_ts

    if eq_start_idx is not None:
        eq_dur = (parsed[-1][0] - parsed[eq_start_idx][0]).total_seconds()
        if eq_dur > m_sec:
            if parsed[eq_start_idx][0] > seg_start_ts:
                intervals.append((seg_start_val, seg_start_ts,
                                  parsed[eq_start_idx][2], parsed[eq_start_idx][0]))
            seg_start_ts = parsed[-1][0]

    if parsed[-1][0] > seg_start_ts:
        intervals.append((seg_start_val, seg_start_ts, prev_val, parsed[-1][0]))
    return intervals


def _connect_package():
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(DB_FILE))
    conn.execute("""
        CREATE TABLE IF NOT EXISTS package_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            api_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            h5_percent REAL,
            h5_reset INTEGER,
            weekly_percent REAL,
            weekly_reset INTEGER,
            monthly_percent REAL,
            monthly_reset INTEGER,
            service_status TEXT
        )
    """)
    # migrate: add service_status column if missing from older DB
    try:
        conn.execute("ALTER TABLE package_history ADD COLUMN service_status TEXT")
    except sqlite3.OperationalError:
        pass
    conn.commit()
    return conn

def save_package_record(api_id: str, h5_percent: float | None, h5_reset: int | None, weekly_percent: float | None, weekly_reset: int | None, monthly_percent: float | None, monthly_reset: int | None, service_status: str | None = None):
    try:
        conn = _connect_package()
        ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
        conn.execute(
            "INSERT INTO package_history (api_id, timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset, service_status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (api_id, ts, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset, service_status),
        )
        conn.commit()
        conn.close()
    except Exception as e:
        log(f"Failed to save package record: {e}")

def get_package_history_page(limit: int = 100, offset: int = 0, api_id: str | None = None):
    try:
        conn = _connect_package()
        if api_id:
            cur = conn.execute("SELECT timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset, service_status FROM package_history WHERE api_id=? ORDER BY timestamp DESC LIMIT ? OFFSET ?", (api_id, limit, offset))
        else:
            cur = conn.execute("SELECT timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset, service_status FROM package_history ORDER BY timestamp DESC LIMIT ? OFFSET ?", (limit, offset))
        rows = [{"timestamp": r[0], "h5_percent": r[1], "h5_reset": r[2], "weekly_percent": r[3], "weekly_reset": r[4], "monthly_percent": r[5], "monthly_reset": r[6], "service_status": r[7] if len(r) > 7 else None} for r in cur.fetchall()]
        # handle case with api_id column
        if rows and len(cur.description) == 8:
            # when api_id requested, the above query without api_id includes it
            pass
        conn.close()
        return rows
    except Exception as e:
        log(f"Failed to read package history: {e}")
        return []

def get_latest_package(api_id: str):
    try:
        conn = _connect_package()
        cur = conn.execute("SELECT timestamp, h5_percent, h5_reset, weekly_percent, weekly_reset, monthly_percent, monthly_reset FROM package_history WHERE api_id=? ORDER BY timestamp DESC LIMIT 1", (api_id,))
        r = cur.fetchone()
        conn.close()
        if r:
            return {"timestamp": r[0], "h5_percent": r[1], "h5_reset": r[2], "weekly_percent": r[3], "weekly_reset": r[4], "monthly_percent": r[5], "monthly_reset": r[6]}
        return None
    except Exception as e:
        log(f"Failed to read latest package: {e}")
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
        conn.close()
        log(f"Pruned balance history older than {retention_days} days")
    except Exception as e:
        log(f"Failed to prune balance history: {e}")

    try:
        conn = _connect_package()
        conn.execute("DELETE FROM package_history WHERE timestamp < datetime('now', ?)", (f"-{retention_days} days",))
        conn.commit()
        conn.close()
        log(f"Pruned package history older than {retention_days} days")
    except Exception as e:
        log(f"Failed to prune package history: {e}")

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
