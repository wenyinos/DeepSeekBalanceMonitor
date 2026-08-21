"""
Platform registry — defines supported platforms, their default mode,
credential fields, and fetch logic.

To add a new platform:
  1. Add an entry to PLATFORMS dict below
  2. If payg: implement fetch in tray_app's _fetch_payg API
  3. If package: implement fetch in tray_app's _fetch_package API
  4. Add i18n keys to config.py _T
"""
from dataclasses import dataclass, field

@dataclass
class PlatformMeta:
    key: str                   # internal id, e.g. "deepseek"
    display_name: str          # shown in UI, e.g. "DeepSeek"
    default_mode: str          # "payg" or "package"
    supports_payg: bool = True
    supports_package: bool = True
    cred_fields: list = field(default_factory=list)
    console_url: str = ""
    # Package-specific: which windows to display
    # "5h"=rolling, "weekly", "monthly"
    package_windows: list = field(default_factory=lambda: ["5h", "weekly", "monthly"])
    # Does this platform have a status page? (affects history table status column)
    has_status_page: bool = False


PLATFORMS = {
    "deepseek": PlatformMeta(
        key="deepseek",
        display_name="DeepSeek",
        default_mode="payg",
        supports_payg=True,
        supports_package=False,
        console_url="https://platform.deepseek.com",
        has_status_page=True,
    ),
    "opencode_go": PlatformMeta(
        key="opencode_go",
        display_name="OpenCode Go",
        default_mode="package",
        supports_payg=False,
        supports_package=True,
        console_url="https://opencode.ai/auth",
        package_windows=["5h", "weekly", "monthly"],
        has_status_page=False,
    ),
    "minimax_token_cn": PlatformMeta(
        key="minimax_token_cn",
        display_name="MiniMax Token Plan (CN)",
        default_mode="package",
        supports_payg=False,
        supports_package=True,
        console_url="https://platform.minimaxi.com",
        package_windows=["5h", "weekly"],
        has_status_page=True,
    ),
    "minimax_token_global": PlatformMeta(
        key="minimax_token_global",
        display_name="MiniMax Token Plan (Global)",
        default_mode="package",
        supports_payg=False,
        supports_package=True,
        console_url="https://platform.minimax.io",
        package_windows=["5h", "weekly"],
        has_status_page=True,
    ),
    "minimax_coding_cn": PlatformMeta(
        key="minimax_coding_cn",
        display_name="MiniMax Coding Plan (CN)",
        default_mode="package",
        supports_payg=False,
        supports_package=True,
        console_url="https://platform.minimaxi.com",
        package_windows=["5h", "weekly"],
        has_status_page=True,
    ),
    "minimax_coding_global": PlatformMeta(
        key="minimax_coding_global",
        display_name="MiniMax Coding Plan (Global)",
        default_mode="package",
        supports_payg=False,
        supports_package=True,
        console_url="https://platform.minimax.io",
        package_windows=["5h", "weekly"],
        has_status_page=True,
    ),
}

def get_platform(key: str) -> PlatformMeta | None:
    return PLATFORMS.get(key)

def get_all_platforms() -> list[PlatformMeta]:
    return list(PLATFORMS.values())
