"""
Command Code (commandcode.ai) quota client — 5h / weekly / monthly windows.

Two plan kinds (registered as separate platforms):
  command_code_goat: GOAT plan — $10/mo for $70 monthly credits, unused roll over.
    Monthly window is ESTIMATED: API only returns credits.monthlyCredits (USD
    remaining); cap is the known GOAT constant 70. Because credits roll over,
    remaining% may EXCEED 100% — we keep that value instead of clamping (app
    convention: track REMAINING %, unlike the Rust build which shows used/cap).
  command_code: standard plan — only 5h + weekly windows (like MiniMax),
    no monthly window.

Endpoints:
  GET https://api.commandcode.ai/alpha/whoami          → orgId (optional)
  GET https://api.commandcode.ai/alpha/billing/credits → quota windows
Auth: Authorization: Bearer <api_key>
"""
import urllib.error

from src.platforms._http import install_proxy as _install_proxy
from src.platforms._http import http_get_json

USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"

API_BASE = "https://api.commandcode.ai/"
GOAT_MONTHLY_CREDITS = 70.0


def _is_goat_plan(plan_id):
    return bool(plan_id) and plan_id.replace("_", "-").lower().startswith("individual-goat")


def _window_remaining(used, cap):
    """5h/weekly windows: remaining% from used/cap, clamped to [0,100]."""
    if not cap or cap <= 0:
        return None
    used = max(0.0, used)
    remaining_pct = max(0.0, (cap - used) / cap * 100.0)
    remaining_pct = min(100.0, remaining_pct)
    return remaining_pct


def _epoch_to_reset_sec(epoch):
    """resetAt may be epoch seconds or milliseconds; returns seconds remaining."""
    if epoch is None:
        return 0
    from datetime import datetime, timezone
    value = float(epoch)
    if value >= 100_000_000_000.0:
        value /= 1000.0
    return max(0, int(value - datetime.now(timezone.utc).timestamp()))


def fetch_command_code_quota(api_key: str, platform_key: str = "command_code_goat",
                             http_proxy: str = "") -> dict:
    """Fetch Command Code quota.

    Returns dict shaped like other package clients:
        {"5h": {"usage_percent","percent_remaining","reset_in_sec"}, "weekly": ..., "monthly": ...}
    Any window absent from the API response is None. percent_remaining may
    exceed 100% for the GOAT monthly estimate (rolled-over credits).

    Raises ValueError on failure (401 → Invalid API key).
    """
    if not api_key or not api_key.strip():
        raise ValueError("No API key provided for Command Code")
    _install_proxy(http_proxy or "")
    headers = {
        "Authorization": f"Bearer {api_key.strip()}",
        "Accept": "application/json",
        "User-Agent": USER_AGENT,
    }

    # optional orgId (whoami failures are tolerated — org-less queries are valid)
    org_id = None
    try:
        who = http_get_json(API_BASE + "alpha/whoami", headers=headers, timeout=10)
        org = who.get("org") or {}
        org_id = org.get("id") or None
    except Exception:
        org_id = None

    url = API_BASE + "alpha/billing/credits"
    if org_id:
        from urllib.parse import quote
        url += "?orgId=" + quote(org_id)

    try:
        data = http_get_json(url, headers=headers, timeout=10)
    except urllib.error.HTTPError as e:
        if e.code == 401:
            raise ValueError("Invalid API key (401)")
        raise ValueError(f"Command Code API error: HTTP {e.code}")
    if not isinstance(data, dict):
        raise ValueError("Command Code API returned an unexpected payload")

    credits = data.get("credits") or {}
    limits = data.get("windowLimits") or {}
    plan_id = credits.get("planId")

    result = {"5h": None, "weekly": None, "monthly": None}
    for key, src in (("5h", limits.get("fiveHour")), ("weekly", limits.get("weekly"))):
        if src is None:
            continue
        used = float(src.get("used") or 0)
        cap = float(src.get("cap") or 0)
        rem = _window_remaining(used, cap)
        if rem is None:
            continue
        result[key] = {
            "usage_percent": 100.0 - rem,
            "percent_remaining": rem,
            "reset_in_sec": _epoch_to_reset_sec(src.get("resetAt")),
        }

    # monthly: GOAT platform only, and planId must confirm GOAT
    goat_mode = platform_key == "command_code_goat"
    monthly_remaining = credits.get("monthlyCredits")
    if goat_mode and _is_goat_plan(plan_id) and monthly_remaining is not None:
        remaining = float(monthly_remaining)
        # NOT clamped: rolled-over credits legitimately exceed 100% remaining
        remaining_pct = remaining / GOAT_MONTHLY_CREDITS * 100.0
        result["monthly"] = {
            "usage_percent": max(0.0, 100.0 - remaining_pct),
            "percent_remaining": remaining_pct,
            "reset_in_sec": 0,
        }

    if result["5h"] is None and result["weekly"] is None and result["monthly"] is None:
        raise ValueError("Command Code API returned no usage windows")
    return result