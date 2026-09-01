#!/usr/bin/env sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This installer must run as root for the system-level binary/service. Use: sudo ./install.sh" >&2
    exit 1
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN_SRC="$SCRIPT_DIR/dsmon"
SERVICE_SRC="$SCRIPT_DIR/dsmon.service"
PLASMOID_SRC="$SCRIPT_DIR/plasmoid"
SERVICE_NAME="dsmon.service"

INSTALL_USER="${SUDO_USER:-$(id -un)}"
if [ "$INSTALL_USER" = "root" ] || [ -z "$INSTALL_USER" ]; then
    echo "Unable to determine the target user (SUDO_USER empty and current user is root)." >&2
    echo "Run the installer via sudo as a normal user, e.g.: sudo ./install.sh" >&2
    exit 1
fi
USER_HOME="$(getent passwd "$INSTALL_USER" | cut -d: -f6)"
if [ -z "$USER_HOME" ]; then
    echo "Unable to determine home directory for user $INSTALL_USER." >&2
    exit 1
fi

# 二进制与服务安装到系统级（需要 root）；Plasma 小组件与图标安装到用户级
BIN_DST="/usr/local/bin/dsmon"
SERVICE_DST="/etc/systemd/user/dsmon.service"
PLASMOID_DST="$USER_HOME/.local/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
ICON_DST="$USER_HOME/.local/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"
ICON_CACHE_DIR="$USER_HOME/.local/share/icons/hicolor"

# 旧版本安装到用户级的 dsmon 残留（1.4.1 曾纯用户级安装，二进制在 ~/.local/bin）
OLD_USER_BIN="$USER_HOME/.local/bin/dsmon"
OLD_USER_SERVICE="$USER_HOME/.config/systemd/user/dsmon.service"
# 1.4.0 及更早安装到系统级的小组件/图标残留（会遮蔽用户级小组件）
OLD_SYSTEM_PLASMOID="/usr/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
OLD_SYSTEM_ICON="/usr/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"

echo "Installs dsmon binary + systemd service to system-level (/usr/local/bin, /etc/systemd/user)."
echo "Plasma widget + icon are installed to your user directory ($USER_HOME)."

# 检测旧用户级残留：如存在则询问是否清理
if [ -e "$OLD_USER_BIN" ] || [ -e "$OLD_USER_SERVICE" ]; then
    echo ""
    echo "NOTE: leftover user-level files from the previous user-only install were detected:"
    [ -e "$OLD_USER_BIN" ] && echo "  - $OLD_USER_BIN"
    [ -e "$OLD_USER_SERVICE" ] && echo "  - $OLD_USER_SERVICE"
    printf "Remove them now? [y/N] "
    IFS= read -r answer || answer=""
    case "$answer" in
        y|Y|yes|YES|Yes)
            if [ -e "$OLD_USER_SERVICE" ] && command -v runuser >/dev/null 2>&1; then
                install_uid="$(id -u "$INSTALL_USER" 2>/dev/null || true)"
                if [ -n "$install_uid" ] && [ -d "/run/user/$install_uid" ]; then
                    runuser -u "$INSTALL_USER" -- env XDG_RUNTIME_DIR="/run/user/$install_uid" systemctl --user disable --now "$SERVICE_NAME" >/dev/null 2>&1 || true
                fi
            fi
            [ -e "$OLD_USER_SERVICE" ] && rm -f "$OLD_USER_SERVICE"
            [ -e "$OLD_USER_BIN" ] && rm -f "$OLD_USER_BIN"
            echo "Old user-level files removed."
            ;;
        *)
            echo "Skipped. They will not interfere with the system-level install."
            ;;
    esac
fi

# 检测 1.4.0 及更早安装到系统级的小组件/图标残留：会遮蔽用户级小组件
OLD_SYSTEM_FOUND=0
[ -d "$OLD_SYSTEM_PLASMOID" ] && OLD_SYSTEM_FOUND=1
[ -f "$OLD_SYSTEM_ICON" ] && OLD_SYSTEM_FOUND=1
if [ "$OLD_SYSTEM_FOUND" -eq 1 ]; then
    echo ""
    echo "NOTE: older system-level widget/icon files from v1.4.0 and earlier were detected:"
    [ -d "$OLD_SYSTEM_PLASMOID" ] && echo "  - $OLD_SYSTEM_PLASMOID"
    [ -f "$OLD_SYSTEM_ICON" ] && echo "  - $OLD_SYSTEM_ICON"
    echo "They can shadow the user-level widget installed below. Remove them?"
    printf "Remove now? [y/N] "
    IFS= read -r answer || answer=""
    case "$answer" in
        y|Y|yes|YES|Yes)
            [ -d "$OLD_SYSTEM_PLASMOID" ] && rm -rf "$OLD_SYSTEM_PLASMOID"
            [ -f "$OLD_SYSTEM_ICON" ] && rm -f "$OLD_SYSTEM_ICON"
            echo "Old system-level widget/icon files removed."
            ;;
        *)
            echo "Skipped. If the widget shows stale UI, remove them later with:"
            echo "  sudo rm -rf $OLD_SYSTEM_PLASMOID"
            echo "  sudo rm -f $OLD_SYSTEM_ICON"
            ;;
    esac
fi

# 检测旧系统级服务是否在运行（当前方案目标位置；覆盖前先停旧服务避免冲突）
if [ -e "$SERVICE_DST" ] && command -v systemctl >/dev/null 2>&1; then
    if [ -n "$INSTALL_USER" ] && [ "$INSTALL_USER" != "root" ] && command -v runuser >/dev/null 2>&1; then
        install_uid="$(id -u "$INSTALL_USER" 2>/dev/null || true)"
        if [ -n "$install_uid" ] && [ -d "/run/user/$install_uid" ]; then
            if runuser -u "$INSTALL_USER" -- env XDG_RUNTIME_DIR="/run/user/$install_uid" systemctl --user is-active "$SERVICE_NAME" >/dev/null 2>&1; then
                echo ""
                echo "An existing $SERVICE_NAME is active. Stopping it before reinstall..."
                runuser -u "$INSTALL_USER" -- env XDG_RUNTIME_DIR="/run/user/$install_uid" systemctl --user stop "$SERVICE_NAME" >/dev/null 2>&1 || true
            fi
        fi
    fi
fi

is_plasma6_session() {
    if [ "${KDE_SESSION_VERSION:-}" = "6" ]; then
        return 0
    fi
    if command -v pgrep >/dev/null 2>&1 && command -v plasmashell >/dev/null 2>&1; then
        if pgrep -u "$INSTALL_USER" -x plasmashell >/dev/null 2>&1; then
            case "$(plasmashell --version 2>/dev/null)" in
                *" 6."*|*" 7."*) return 0 ;;
            esac
        fi
    fi
    case ":${XDG_CURRENT_DESKTOP:-}:${DESKTOP_SESSION:-}:" in
        *KDE*|*kde*|*Plasma*|*plasma*) ;;
        *) return 1 ;;
    esac
    if command -v plasmashell >/dev/null 2>&1; then
        case "$(plasmashell --version 2>/dev/null)" in
            *" 6."*|*" 7."*) return 0 ;;
        esac
    fi
    return 1
}

should_install_plasmoid() {
    if is_plasma6_session; then
        return 0
    fi
    printf "Plasma 6 desktop session was not detected. Install Plasma widget anyway? [y/N] "
    IFS= read -r answer || answer=""
    case "$answer" in
        y|Y|yes|YES|Yes) return 0 ;;
        *) return 1 ;;
    esac
}

if [ ! -f "$BIN_SRC" ]; then
    echo "Missing dsmon binary next to install.sh" >&2
    exit 1
fi

INSTALL_PLASMOID=0
if should_install_plasmoid; then
    INSTALL_PLASMOID=1
    if [ ! -f "$PLASMOID_SRC/metadata.json" ]; then
        echo "Missing Plasma widget package next to install.sh" >&2
        exit 1
    fi
    if [ ! -f "$PLASMOID_SRC/contents/images/deepseek-balance-monitor.png" ]; then
        echo "Missing Plasma widget icon next to install.sh" >&2
        exit 1
    fi
fi

# 安装二进制（系统级）
install -D -m 755 "$BIN_SRC" "$BIN_DST"

# 安装 systemd 用户服务（系统级，ExecStart 用 /usr/local/bin/dsmon）
install -D -m 644 "$SERVICE_SRC" "$SERVICE_DST"

if [ "$INSTALL_PLASMOID" -eq 1 ]; then
    # 安装 plasmoid（用户级）
    install -d -m 755 "$(dirname "$PLASMOID_DST")"
    rm -rf "$PLASMOID_DST"
    cp -R "$PLASMOID_SRC" "$PLASMOID_DST"
    chown -R "$INSTALL_USER:" "$PLASMOID_DST"

    # 安装图标（用户级）
    install -d -m 755 "$(dirname "$ICON_DST")"
    install -m 644 "$PLASMOID_SRC/contents/images/deepseek-balance-monitor.png" "$ICON_DST"
    chown "$INSTALL_USER:" "$ICON_DST"

    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d "$ICON_CACHE_DIR" ]; then
        gtk-update-icon-cache -q "$ICON_CACHE_DIR" 2>/dev/null || true
    fi
fi

echo ""
echo "Installed $BIN_DST"
echo "Installed $SERVICE_DST"
if [ "$INSTALL_PLASMOID" -eq 1 ]; then
    echo "Installed Plasma widget (user-level): $PLASMOID_DST"
    echo "Installed Plasma widget icon (user-level): $ICON_DST"
else
    echo "Skipped Plasma widget installation"
fi
echo ""

# 自动设为自启动：systemd 下 enable --now（用户级）
if command -v systemctl >/dev/null 2>&1; then
    echo "Reloading user systemd manager..."
    if [ -n "$INSTALL_USER" ] && [ "$INSTALL_USER" != "root" ] && command -v runuser >/dev/null 2>&1; then
        install_uid="$(id -u "$INSTALL_USER" 2>/dev/null || true)"
        if [ -n "$install_uid" ] && [ -d "/run/user/$install_uid" ]; then
            echo "Enabling and starting $SERVICE_NAME for $INSTALL_USER..."
            if runuser -u "$INSTALL_USER" -- env XDG_RUNTIME_DIR="/run/user/$install_uid" systemctl --user daemon-reload; then
                if runuser -u "$INSTALL_USER" -- env XDG_RUNTIME_DIR="/run/user/$install_uid" systemctl --user enable --now "$SERVICE_NAME" 2>/dev/null; then
                    echo "$SERVICE_NAME enabled (auto-start on login) and started."
                else
                    echo "Failed to enable/start $SERVICE_NAME automatically."
                    echo "Do it manually as $INSTALL_USER: systemctl --user enable --now $SERVICE_NAME"
                fi
            fi
        else
            echo "No active user session found; reload manually as $INSTALL_USER:"
            echo "  systemctl --user daemon-reload"
        fi
    else
        echo "Reload user systemd manually if needed: systemctl --user daemon-reload"
    fi
else
    echo "No systemd detected. Start the daemon manually with: $BIN_DST daemon"
fi

# 首次查询并提示设置 API key
prompt_api_key() {
    if [ ! -t 0 ]; then
        echo "Set it with: dsmon set-key <api_key>"
        return
    fi
    printf "Enter DeepSeek API key now (leave blank to skip): "
    HIDE_INPUT=0
    if stty -echo 2>/dev/null; then
        HIDE_INPUT=1
    fi
    IFS= read -r API_KEY || API_KEY=""
    if [ "$HIDE_INPUT" -eq 1 ]; then
        stty echo
    fi
    printf "\n"
    if [ -z "$API_KEY" ]; then
        echo "Skipped API key setup. Set it later with: dsmon set-key <api_key>"
        return
    fi
    if printf "%s\n" "$API_KEY" | run_dsmon_for_user set-key; then
        echo "Running check after saving API key..."
        if ! run_dsmon_for_user check; then
            echo "API key was saved, but the check still failed. Please review the output above."
        fi
    else
        echo "Failed to save API key. Set it later with: dsmon set-key <api_key>" >&2
    fi
}
run_dsmon_for_user() {
    if command -v runuser >/dev/null 2>&1 && [ "$INSTALL_USER" != "root" ]; then
        runuser -u "$INSTALL_USER" -- "$BIN_DST" "$@"
    else
        "$BIN_DST" "$@"
    fi
}
echo "Running first check..."
CHECK_STATUS=0
CHECK_OUTPUT="$(run_dsmon_for_user check 2>&1)" || CHECK_STATUS=$?
if [ -n "$CHECK_OUTPUT" ]; then
    printf "%s\n" "$CHECK_OUTPUT"
fi
if [ "$CHECK_STATUS" -eq 2 ]; then
    prompt_api_key
elif [ "$CHECK_STATUS" -ne 0 ]; then
    case "$CHECK_OUTPUT" in
        *"Invalid API key"*|*"401 Unauthorized"*) prompt_api_key ;;
        *) echo "First check failed for a non-key reason. Configure the API key later with: dsmon set-key <api_key>" ;;
    esac
fi
echo ""
if command -v systemctl >/dev/null 2>&1; then
    echo "Installation complete. $SERVICE_NAME is set to auto-start on login for $INSTALL_USER."
    echo "If it is not running, start it now: systemctl --user start dsmon.service"
else
    echo "Installation complete. Start the daemon manually with: $BIN_DST daemon"
fi
if [ "$INSTALL_PLASMOID" -eq 1 ]; then
    echo "Add widget: right-click panel/desktop -> Add Widgets -> DeepSeek Balance Monitor"
    echo "If the old Plasma widget UI or icon is still shown, restart plasmashell or log out and log back in."
fi
