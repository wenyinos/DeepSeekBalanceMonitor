"""Shared HTTP / formatting helpers for platform clients."""
import json
import socket
import urllib.request

socket.setdefaulttimeout(15)


def install_proxy(proxy_url: str):
    """Install a global HTTP/HTTPS opener. Empty url = bypass system proxy."""
    if proxy_url:
        handler = urllib.request.ProxyHandler({"http": proxy_url, "https": proxy_url})
        urllib.request.install_opener(urllib.request.build_opener(handler))
    else:
        urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))


def http_get_json(url: str, headers: dict | None = None, timeout: int = 15) -> dict:
    """GET a JSON endpoint. Returns parsed dict; raises HTTPError on 4xx/5xx,
    URLError on network failure."""
    req = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def format_reset_short(sec: int, lang: str = "zh") -> str:
    """Compact reset countdown, e.g. `3天2小时重置` / `2d5h`."""
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
