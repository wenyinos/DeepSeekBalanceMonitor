//! The platforms the application knows about.
//!
//! Each entry describes what the platform offers, which is what lets the
//! interface decide how to render an account: a balance card for pay-as-you-go,
//! quota bars for a subscription.
//!
//! `implemented` marks whether the client exists yet. Unimplemented entries are
//! listed so the settings page can already accept their keys and so the data
//! model does not have to change when the client lands. See
//! `PLATFORM_PORTING.md` for the interface details of each one.

/// What kind of reading a platform reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// An account balance, in currency — DeepSeek, Kimi, StepFun, OpenRouter.
    Payg,
    /// Subscription quota windows, as percentages — OpenCode Go, Command Code.
    Package,
}

impl Mode {
    pub fn from_config(value: &str) -> Option<Self> {
        match value {
            "payg" => Some(Mode::Payg),
            "package" => Some(Mode::Package),
            _ => None,
        }
    }

    pub fn as_config(self) -> &'static str {
        match self {
            Mode::Payg => "payg",
            Mode::Package => "package",
        }
    }
}

/// A platform the application knows about.
#[derive(Debug, Clone, Copy)]
pub struct PlatformMeta {
    /// Stable identifier: used in the configuration, and as the name its secret
    /// is stored under.
    pub key: &'static str,
    /// Shown in the interface.
    pub display_name: &'static str,
    pub mode: Mode,
    /// Quota windows for package platforms, in display order.
    pub windows: &'static [&'static str],
    /// Where the user manages the account.
    pub console_url: &'static str,
    /// Whether the client for this platform is written yet.
    pub implemented: bool,
}

/// Every platform the application lists, implemented or not.
pub const PLATFORMS: [PlatformMeta; 14] = [
    PlatformMeta {
        key: "deepseek",
        display_name: "DeepSeek",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://platform.deepseek.com",
        implemented: true,
    },
    PlatformMeta {
        key: "opencode_go",
        display_name: "OpenCode Go",
        mode: Mode::Package,
        windows: &["5h", "weekly", "monthly"],
        console_url: "https://opencode.ai/auth",
        implemented: true,
    },
    PlatformMeta {
        key: "command_code",
        display_name: "Command Code",
        mode: Mode::Package,
        windows: &["5h", "weekly", "monthly"],
        console_url: "https://commandcode.ai",
        implemented: true,
    },
    PlatformMeta {
        key: "glm_coding_cn",
        display_name: "GLM Coding Plan (CN)",
        mode: Mode::Package,
        windows: &["5h", "weekly", "monthly"],
        console_url: "https://open.bigmodel.cn",
        implemented: true,
    },
    PlatformMeta {
        key: "glm_coding_global",
        display_name: "GLM Coding Plan (Global)",
        mode: Mode::Package,
        windows: &["5h", "weekly", "monthly"],
        console_url: "https://z.ai",
        implemented: true,
    },
    PlatformMeta {
        key: "kimi_token_cn",
        display_name: "Kimi (CN)",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://platform.moonshot.cn",
        implemented: true,
    },
    PlatformMeta {
        key: "kimi_token_global",
        display_name: "Kimi (Global)",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://platform.moonshot.ai",
        implemented: true,
    },
    PlatformMeta {
        key: "minimax_token_cn",
        display_name: "MiniMax Token Plan (CN)",
        mode: Mode::Package,
        windows: &["5h", "weekly"],
        console_url: "https://platform.minimaxi.com",
        implemented: true,
    },
    PlatformMeta {
        key: "minimax_token_global",
        display_name: "MiniMax Token Plan (Global)",
        mode: Mode::Package,
        windows: &["5h", "weekly"],
        console_url: "https://platform.minimax.io",
        implemented: true,
    },
    PlatformMeta {
        key: "minimax_coding_cn",
        display_name: "MiniMax Coding Plan (CN)",
        mode: Mode::Package,
        windows: &["5h", "weekly"],
        console_url: "https://platform.minimaxi.com",
        implemented: true,
    },
    PlatformMeta {
        key: "minimax_coding_global",
        display_name: "MiniMax Coding Plan (Global)",
        mode: Mode::Package,
        windows: &["5h", "weekly"],
        console_url: "https://platform.minimax.io",
        implemented: true,
    },
    PlatformMeta {
        key: "stepfun_token_cn",
        display_name: "StepFun (CN)",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://platform.stepfun.com",
        implemented: true,
    },
    PlatformMeta {
        key: "stepfun_token_global",
        display_name: "StepFun (Global)",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://platform.stepfun.ai",
        implemented: true,
    },
    PlatformMeta {
        key: "openrouter",
        display_name: "OpenRouter",
        mode: Mode::Payg,
        windows: &[],
        console_url: "https://openrouter.ai/settings/keys",
        implemented: true,
    },
];

/// Currencies reported by platforms that only deal in one.
pub fn default_currency(key: &str) -> &'static str {
    match key {
        "kimi_token_cn" | "stepfun_token_cn" => "CNY",
        "kimi_token_global" | "stepfun_token_global" | "openrouter" => "USD",
        _ => "CNY",
    }
}

/// Looks a platform up by its identifier.
pub fn find(key: &str) -> Option<&'static PlatformMeta> {
    PLATFORMS.iter().find(|meta| meta.key == key)
}

/// The platforms whose client exists.
pub fn implemented() -> impl Iterator<Item = &'static PlatformMeta> {
    PLATFORMS.iter().filter(|meta| meta.implemented)
}

/// The platforms still waiting for a client.
pub fn pending() -> impl Iterator<Item = &'static PlatformMeta> {
    PLATFORMS.iter().filter(|meta| !meta.implemented)
}

/// Label key for a window, for the text table.
pub fn window_label_key(window: &str) -> &'static str {
    match window {
        "5h" => "window_5h",
        "weekly" => "window_weekly",
        "monthly" => "window_monthly",
        _ => "window_unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platforms_are_findable_by_key() {
        assert_eq!(find("deepseek").map(|meta| meta.mode), Some(Mode::Payg));
        assert_eq!(
            find("opencode_go").map(|meta| meta.mode),
            Some(Mode::Package)
        );
        assert!(find("nonsense").is_none());
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = PLATFORMS.iter().map(|meta| meta.key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "a platform key is duplicated");
    }

    #[test]
    fn package_platforms_declare_windows() {
        for meta in PLATFORMS {
            match meta.mode {
                Mode::Package => assert!(
                    !meta.windows.is_empty(),
                    "{} needs at least one window",
                    meta.key
                ),
                Mode::Payg => assert!(
                    meta.windows.is_empty(),
                    "{} has no windows to show",
                    meta.key
                ),
            }
        }
    }

    #[test]
    fn every_listed_platform_has_a_client() {
        let implemented: Vec<&str> = implemented().map(|meta| meta.key).collect();
        assert_eq!(
            implemented,
            [
                "deepseek",
                "opencode_go",
                "command_code",
                "glm_coding_cn",
                "glm_coding_global",
                "kimi_token_cn",
                "kimi_token_global",
                "minimax_token_cn",
                "minimax_token_global",
                "minimax_coding_cn",
                "minimax_coding_global",
                "stepfun_token_cn",
                "stepfun_token_global",
                "openrouter",
            ],
            "the list is what the interface offers; a new client belongs here"
        );
        assert_eq!(pending().count(), 0, "nothing is waiting for a client");
    }

    #[test]
    fn modes_round_trip_through_the_config() {
        for mode in [Mode::Payg, Mode::Package] {
            assert_eq!(Mode::from_config(mode.as_config()), Some(mode));
        }
        assert_eq!(Mode::from_config("nonsense"), None);
    }
}
