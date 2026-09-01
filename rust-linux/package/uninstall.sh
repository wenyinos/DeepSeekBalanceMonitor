#!/usr/bin/env sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This uninstaller must run as root for the system-level binary/service. Use: sudo ./uninstall.sh" >&2
    exit 1
fi

SERVICE_NAME="dsmon.service"
BIN_DST="/usr/local/bin/dsmon"
SERVICE_DST="/etc/systemd/user/$SERVICE_NAME"

INSTALL_USER="${SUDO_USER:-$(id -un)}"
if [ -n "$INSTALL_USER" ] && [ "$INSTALL_USER" != "root" ]; then
    USER_HOME="$(getent passwd "$INSTALL_USER" | cut -d: -f6)"
else
    USER_HOME=""
fi
# 小组件与图标安装在用户级目录
PLASMOID_DST=""
ICON_DST=""
ICON_CACHE_DIR=""
if [ -n "$USER_HOME" ]; then
    PLASMOID_DST="$USER_HOME/.local/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
    ICON_DST="$USER_HOME/.local/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"
    ICON_CACHE_DIR="$USER_HOME/.local/share/icons/hicolor"
fi
# 旧版本残留：用户级二进制（1.4.1 纯用户级安装）与旧系统级小组件
OLD_USER_BIN="$USER_HOME/.local/bin/dsmon"
OLD_USER_SERVICE="$USER_HOME/.config/systemd/user/dsmon.service"
OLD_SYSTEM_PLASMOID="/usr/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
OLD_SYSTEM_ICON="/usr/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"

CORE_INSTALLED=0
if [ -e "$BIN_DST" ] || [ -e "$SERVICE_DST" ] || [ -e "$OLD_USER_BIN" ] || [ -e "$OLD_USER_SERVICE" ]; then
    CORE_INSTALLED=1
fi

PLASMA_FILES_FOUND=0
if { [ -n "$PLASMOID_DST" ] && [ -d "$PLASMOID_DST" ]; } || { [ -n "$ICON_DST" ] && [ -f "$ICON_DST" ]; } || [ -d "$OLD_SYSTEM_PLASMOID" ] || [ -f "$OLD_SYSTEM_ICON" ]; then
    PLASMA_FILES_FOUND=1
fi

if [ "$CORE_INSTALLED" -eq 0 ]; then
    echo "dsmon executable and systemd service were not found. Core app is not installed."
    if [ "$PLASMA_FILES_FOUND" -eq 0 ]; then
        echo "Nothing to uninstall."
        exit 0
    fi
fi

run_user_systemctl() {
    service_user="$1"
    shift
    service_uid="$(id -u "$service_user" 2>/dev/null || true)"
    if [ -z "$service_uid" ] || [ ! -d "/run/user/$service_uid" ]; then
        echo "Skipping user systemd action for $service_user; no active user session was found."
        return
    fi
    if command -v runuser >/dev/null 2>&1; then
        runuser -u "$service_user" -- env XDG_RUNTIME_DIR="/run/user/$service_uid" systemctl --user "$@" || true
    else
        echo "runuser is not available. Run manually as $service_user: systemctl --user $*"
    fi
}

if [ "$CORE_INSTALLED" -eq 1 ]; then
    if [ -n "$INSTALL_USER" ] && [ "$INSTALL_USER" != "root" ]; then
        echo "Stopping and disabling $SERVICE_NAME for user $INSTALL_USER..."
        run_user_systemctl "$INSTALL_USER" disable --now "$SERVICE_NAME"
    fi
fi

if [ "$CORE_INSTALLED" -eq 1 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl --global disable "$SERVICE_NAME" >/dev/null 2>&1 || true
fi
if [ -e "$SERVICE_DST" ]; then
    rm -f "$SERVICE_DST"
    echo "Removed $SERVICE_DST"
else
    echo "Not found: $SERVICE_DST"
fi
if [ -e "$BIN_DST" ]; then
    rm -f "$BIN_DST"
    echo "Removed $BIN_DST"
else
    echo "Not found: $BIN_DST"
fi
# 清理旧用户级残留
if [ -e "$OLD_USER_SERVICE" ]; then
    rm -f "$OLD_USER_SERVICE"
    echo "Removed $OLD_USER_SERVICE"
fi
if [ -e "$OLD_USER_BIN" ]; then
    rm -f "$OLD_USER_BIN"
    echo "Removed $OLD_USER_BIN"
fi

if [ "$CORE_INSTALLED" -eq 1 ] && [ -n "$INSTALL_USER" ] && [ "$INSTALL_USER" != "root" ]; then
    run_user_systemctl "$INSTALL_USER" daemon-reload
fi

echo "User data and configuration were not removed."

if [ "$PLASMA_FILES_FOUND" -eq 1 ]; then
    echo ""
    echo "Plasma widget files were detected and left in place intentionally."
    echo "Do not delete the widget directory while the widget is still on your panel or desktop."
    echo "To remove it safely:"
    echo "  1. In Plasma, remove every DeepSeek Balance Monitor widget from panels/desktops."
    echo "  2. Log out and log back in, or restart plasmashell."
    echo "  3. After no widget instance is active, remove the package files manually:"
    if [ -n "$PLASMOID_DST" ]; then
        echo "     rm -rf $PLASMOID_DST"
    fi
    if [ -n "$ICON_DST" ]; then
        echo "     rm -f $ICON_DST"
    fi
    if [ -d "$OLD_SYSTEM_PLASMOID" ]; then
        echo "     sudo rm -rf $OLD_SYSTEM_PLASMOID"
    fi
    if [ -f "$OLD_SYSTEM_ICON" ]; then
        echo "     sudo rm -f $OLD_SYSTEM_ICON"
    fi
    [ -n "$ICON_CACHE_DIR" ] && echo "     gtk-update-icon-cache -q $ICON_CACHE_DIR 2>/dev/null || true"
else
    echo "No Plasma widget package files were detected."
fi
