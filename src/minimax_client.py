"""
MiniMax quota client — fetches Token Plan / Coding Plan usage.

Token Plan:   GET https://www.minimaxi.com/v1/token_plan/remains (CN)
              GET https://www.minimax.io/v1/token_plan/remains (Global)
Coding Plan:  GET https://www.minimaxi.com/v1/api/openplatform/coding_plan/remains (CN)
              GET https://www.minimax.io/v1/api/openplatform/coding_plan/remains (Global)

Response: {"base_resp":{"status_code":0}, "data":{"model_remains":[{"model_name","current_interval_*","current_weekly_*",...}]}}
Fields: current_interval_remaining_percent, current_weekly_remaining_percent, end_time, weekly_end_time.
Note: no monthly window — only 5h rolling + weekly.
"""
import json
import ssl
import time
import urllib.request
import urllib.error
from datetime import datetime, timezone

USER_AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Gecko/20100101 Firefox/148.0"

# Endpoint mapping: platform_key -> (base_url, path)
_MINIMAX_ENDPOINTS = {
    "minimax_token_cn":     ("https://www.minimaxi.com", "/v1/token_plan/remains"),
    "minimax_token_global": ("https://www.minimax.io",    "/v1/token_plan/remains"),
    "minimax_coding_cn":    ("https://www.minimaxi.com", "/v1/api/openplatform/coding_plan/remains"),
    "minimax_coding_global":("https://www.minimax.io",    "/v1/api/openplatform/coding_plan/remains"),
}

def _install_proxy(proxy_url: str):
    if proxy_url:
        handler = urllib.request.ProxyHandler({"http": proxy_url, "https": proxy_url})
        opener = urllib.request.build_opener(handler)
    else:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    urllib.request.install_opener(opener)

def _parse_timestamp(ts):
    """Parse a timestamp that could be seconds or milliseconds."""
    if ts is None:
        return 0
    ts = int(ts)
    # if > 1e12, it's milliseconds
    if ts > 1e12:
        ts = ts // 1000
    return ts

def _parse_model_remains(remains_list, window_key):
    """Find the 'general' or first model entry, extract 5h/weekly data."""
    result = {"5h": None, "weekly": None}
    # prefer "general" model, else first entry
    entry = None
    for r in remains_list:
        name = r.get("model_name", "")
        if name.lower() == "general" or not entry:
            entry = r
            if name.lower() == "general":
                break
    if not entry:
        return result
    # 5h rolling window
    h5_pct = entry.get("current_interval_remaining_percent")
    h5_end = _parse_timestamp(entry.get("end_time"))
    if h5_pct is not None:
        now = datetime.now(timezone.utc)
        reset_sec = max(0, h5_end - int(now.timestamp())) if h5_end else 0
        result["5h"] = {"usage_percent": 100 - h5_pct, "percent_remaining": float(h5_pct), "reset_in_sec": reset_sec}
    # weekly
    wk_pct = entry.get("current_weekly_remaining_percent")
    wk_end = _parse_timestamp(entry.get("weekly_end_time"))
    if wk_pct is not None:
        now = datetime.now(timezone.utc)
        reset_sec = max(0, wk_end - int(now.timestamp())) if wk_end else 0
        result["weekly"] = {"usage_percent": 100 - wk_pct, "percent_remaining": float(wk_pct), "reset_in_sec": reset_sec}
    return result

def fetch_minimax_quota(platform_key: str, api_key: str, http_proxy: str = "") -> dict:
    """Fetch MiniMax quota.

    Returns: {"5h": {"usage_percent","percent_remaining","reset_in_sec"}, "weekly": ...}
    Raises ValueError on failure.
    """
    if not api_key or not api_key.strip():
        raise ValueError("No API key provided for MiniMax")
    endpoint = _MINIMAX_ENDPOINTS.get(platform_key)
    if not endpoint:
        raise ValueError(f"Unknown MiniMax platform: {platform_key}")
    base_url, path = endpoint
    url = base_url + path

    _install_proxy(http_proxy or "")
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {api_key.strip()}",
        "Content-Type": "application/json",
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "Connection": "close",
    })
    # TLS to minimax hosts intermittently drops mid-handshake (SSL UNEXPECTED_EOF);
    # retry transient network errors before giving up
    last_err = None
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                body = resp.read().decode("utf-8")
                data = json.loads(body)
                # check base_resp
                base_resp = data.get("base_resp", {})
                if base_resp.get("status_code", 0) != 0:
                    raise ValueError(base_resp.get("status_msg", "MiniMax API error"))
                # find model_remains — could be under data or at root
                remains = data.get("data", {}).get("model_remains") or data.get("model_remains") or []
                if not remains:
                    raise ValueError("No model_remains data in MiniMax response")
                result = _parse_model_remains(remains, "general")
                if result["5h"] is None and result["weekly"] is None:
                    raise ValueError("No usage data found in MiniMax response")
                return result
        except ValueError:
            raise  # API-level errors are not transient
        except urllib.error.HTTPError as e:
            if e.code == 401:
                raise ValueError("Invalid API key (401)")
            raise ValueError(f"MiniMax API error: HTTP {e.code}")
        except (urllib.error.URLError, ssl.SSLError, ConnectionError, TimeoutError) as e:
            last_err = e
            if attempt < 2:
                time.sleep(1.0)
    raise ValueError(str(last_err))

def format_reset_short(sec: int, lang: str = "zh") -> str:
    if sec <= 0:
        return "—"
    d = sec // 86400
    h = (sec % 86400) // 3600
    m = (sec % 3600) // 60
    if lang == "zh":
        if d > 0:
            return f"{d}天{h}小时重置"
        if h > 0:
            return f"{h}小时{m}分重置"
        return f"{m}分钟后重置"
    else:
        if d > 0:
            return f"{d}d{h}h"
        if h > 0:
            return f"{h}h{m}m"
        return f"{m}m"
