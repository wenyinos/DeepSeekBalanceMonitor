"""
DeepSeek API client — fetches account balance from the DeepSeek API.
"""
import requests


def fetch_balance(api_key: str) -> dict:
    """Query balance. Returns dict with 'is_available' and 'all_balances'.

    Raises PermissionError on 401, requests.HTTPError on other HTTP errors,
    ValueError if the response contains no balance_infos.
    """
    # HTTP headers must be Latin-1 (RFC 7230 §3.2).  Any character
    # outside Latin-1 in the API key will crash http.client.putheader()
    # with UnicodeEncodeError before the request ever leaves the machine.
    api_key = api_key.encode("latin-1", errors="ignore").decode("latin-1")

    url = "https://api.deepseek.com/user/balance"
    headers = {"Accept": "application/json", "Authorization": f"Bearer {api_key}"}
    resp = requests.get(url, headers=headers, timeout=15)
    if resp.status_code == 401:
        raise PermissionError("Invalid API Key (401 Unauthorized)")
    resp.raise_for_status()
    data = resp.json()
    infos = data.get("balance_infos", [])
    if not infos:
        raise ValueError("No balance information in response")
    all_balances = {}
    for info in infos:
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
