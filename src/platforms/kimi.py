"""
Kimi (Moonshot AI) balance client — CN and Global platforms.

CN:     GET https://api.moonshot.cn/v1/users/me/balance   (CNY)
Global: GET https://api.moonshot.ai/v1/users/me/balance   (USD)

Auth: Authorization: Bearer <api_key>
Response: {"code": 0, "status": true,
           "data": {"available_balance": float, "voucher_balance": float, "cash_balance": float}}

Note: keys are platform-bound — a CN key against the Global host returns 401.
"""
from src.platforms._http import install_proxy as _install_proxy
import urllib.error

from src.platforms._http import http_get_json

USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"

_KIMI_ENDPOINTS = {
    "kimi_token_cn":     ("https://api.moonshot.cn", "CNY"),
    "kimi_token_global": ("https://api.moonshot.ai", "USD"),
}


def fetch_kimi_balance(api_key: str, platform_key: str = "kimi_token_cn", http_proxy: str = "") -> dict:
    """Fetch Kimi balance via the official users/me/balance endpoint.

    Returns dict shaped like the DeepSeek client's result so tray/storage can
    treat both payg platforms identically:
        {"is_available": bool,
         "all_balances": {currency: {"total_balance", "granted_balance",
                                     "topped_up_balance"}}}

    Raises ValueError on failure (401 → Invalid API key).
    """
    if not api_key or not api_key.strip():
        raise ValueError("No API key provided for Kimi")
    endpoint = _KIMI_ENDPOINTS.get(platform_key)
    if not endpoint:
        raise ValueError(f"Unknown Kimi platform: {platform_key}")
    base_url, currency = endpoint
    url = base_url + "/v1/users/me/balance"

    _install_proxy(http_proxy or "")
    headers = {
        "Authorization": f"Bearer {api_key.strip()}",
        "Content-Type": "application/json",
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
    }
    try:
        data = http_get_json(url, headers=headers, timeout=10)
    except urllib.error.HTTPError as e:
        if e.code == 401:
            raise ValueError("Invalid API key (401)")
        raise ValueError(f"Kimi API error: HTTP {e.code}")

    code = data.get("code")
    status = data.get("status", False)
    payload = data.get("data") or {}
    if code != 0 or not status:
        raise ValueError(f"Kimi API error: scode={data.get('scode', '?')}")

    available = float(payload.get("available_balance", 0))
    voucher = float(payload.get("voucher_balance", 0))
    cash = float(payload.get("cash_balance", 0))
    return {
        "is_available": available > 0,
        "all_balances": {
            # total=available (cash+voucher); granted=voucher; topped=cash —
            # maps onto the app's three-field balance model
            currency: {
                "total_balance": available,
                "granted_balance": voucher,
                "topped_up_balance": cash,
            }
        },
    }
