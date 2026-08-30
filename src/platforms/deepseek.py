"""
DeepSeek API client — balance + FlashDuty status page.
"""
import re
import urllib.request
import urllib.error

from src.platforms._http import install_proxy, http_get_json


def fetch_balance(api_key: str) -> dict:
    """Query balance. Returns dict with 'is_available' and 'all_balances'.

    Raises PermissionError on 401, ValueError on empty payload,
    URLError/HTTPError on other failures.
    """
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }
    try:
        data = http_get_json("https://api.deepseek.com/user/balance", headers=headers)
    except urllib.error.HTTPError as e:
        if e.code == 401:
            raise PermissionError("Invalid API key (401)")
        raise
    if not data.get("balance_infos"):
        raise ValueError("No balance information returned")

    all_balances = {}
    for info in data.get("balance_infos", []):
        code = info.get("currency", "CNY")
        all_balances[code] = {
            "total_balance": float(info.get("total_balance", 0)),
            "granted_balance": float(info.get("granted_balance", 0)),
            "topped_up_balance": float(info.get("topped_up_balance", 0)),
        }
    return {
        "is_available": data.get("is_available", True),
        "all_balances": all_balances,
    }


# FlashDuty status → legacy indicator mapping
_FLASHDUTY_MAP = {
    "operational":        "none",
    "degraded":           "minor",
    "partial_outage":     "major",
    "full_outage":        "critical",
    "under_maintenance":  "maintenance",
}


def fetch_service_status():
    """Fetch DeepSeek API service status from FlashDuty status page.
    Returns dict {"indicator": str, "api_operational": bool},
    or None on failure."""
    try:
        url = "https://status.flashcat.cloud/deepseek"
        headers = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=5) as resp:
            html = resp.read().decode("utf-8")
        full = " ".join(html.split("\n"))

        # Find component names from RSC payload
        names = re.findall(r'\\"name\\"\s*:\s*\\"((?:API|Web|网页|APP|对话)[^\\]+)\\"', full)
        seen = set()
        api_name = None
        for n in names:
            if n not in seen:
                seen.add(n)
                if n.startswith("API") or "API" in n:
                    api_name = n
                    break

        if not api_name:
            return {"indicator": "none", "api_operational": True}

        # Check active incidents for API component
        active_match = re.search(r'\\"active_changes\\"\s*:\s*(\[[^\]]*\])', full)
        if active_match:
            raw = active_match.group(1).replace("\\", "")
            import json as _json
            changes = _json.loads(raw)
            for inc in changes:
                for ac in inc.get("affected_components", []):
                    if ac.get("name") == api_name:
                        status = ac.get("status", "degraded")
                        indicator = _FLASHDUTY_MAP.get(status, "none")
                        return {"indicator": indicator,
                                "api_operational": status == "operational"}

        return {"indicator": "none", "api_operational": True}
    except Exception:
        return None
