"""
StepFun (阶跃星辰) balance client — CN and Global platforms, payg accounts only.

CN:     GET https://api.stepfun.com/v1/accounts   (CNY)
Global: GET https://api.stepfun.ai/v1/accounts    (USD)

Auth: Authorization: Bearer <api_key>
Response (payg only):
    {"object": "account", "type": "prepaid",
     "balance": float, "total_cash_balance": float, "total_voucher_balance": float}

Note: this endpoint does NOT cover Step Plan subscriptions (credit pool has no
public quota API). `type == "postpaid"` accounts have no prepayment semantics.
"""
import urllib.error

from src.platforms._http import install_proxy as _install_proxy
from src.platforms._http import http_get_json

USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"

_STEPFUN_ENDPOINTS = {
    "stepfun_token_cn":     ("https://api.stepfun.com", "CNY"),
    "stepfun_token_global": ("https://api.stepfun.ai",  "USD"),
}


def fetch_stepfun_balance(api_key: str, platform_key: str = "stepfun_token_cn",
                          http_proxy: str = "") -> dict:
    """Fetch StepFun account balance via the official /v1/accounts endpoint.

    Returns a dict shaped like the app's payg model:
        {"is_available": bool,
         "all_balances": {currency: {"total_balance", "granted_balance",
                                     "topped_up_balance"}}}

    Field mapping (semantics preserved per platform):
        total_balance   ← balance               (available; cash + voucher)
        topped_up_balance ← total_cash_balance  (topped-up)
        granted_balance ← total_voucher_balance (vouchers)

    Raises ValueError on failure (401 → Invalid API key).
    """
    if not api_key or not api_key.strip():
        raise ValueError("No API key provided for StepFun")
    endpoint = _STEPFUN_ENDPOINTS.get(platform_key)
    if not endpoint:
        raise ValueError(f"Unknown StepFun platform: {platform_key}")
    base_url, currency = endpoint
    url = base_url + "/v1/accounts"

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
        raise ValueError(f"StepFun API error: HTTP {e.code}")

    # postpaid accounts carry no prepaid balance semantics we can chart
    acct_type = data.get("type", "prepaid")
    balance = float(data.get("balance", 0))
    cash = float(data.get("total_cash_balance", 0))
    voucher = float(data.get("total_voucher_balance", 0))

    return {
        "is_available": balance > 0,
        "account_type": acct_type,
        "all_balances": {
            currency: {
                "total_balance": balance,
                "topped_up_balance": cash,
                "granted_balance": voucher,
            }
        },
    }
