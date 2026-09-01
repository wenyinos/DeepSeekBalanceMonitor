#!/usr/bin/env sh
set -eu

# 纯用户级卸载：不写任何系统目录，无需 sudo
INSTALL_USER="$(id -un)"
USER_HOME="${HOME:-}"
if [ -z "$USER_HOME" ]; then
    USER_HOME="$(getent passwd "$INSTALL_USER" | cut -d: -f6)"
fi

SERVICE_NAME="dsmon.service"
BIN_DST="$USER_HOME/.local/bin/dsmon"
SERVICE_DST="$USER_HOME/.config/systemd/user/$SERVICE_NAME"
PLASMOID_DST="$USER_HOME/.local/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
ICON_DST="$USER_HOME/.local/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"
ICON_CACHE_DIR="$USER_HOME/.local/share/icons/hicolor"
AUTOSTART_DST="$USER_HOME/.config/autostart/deepseek-balance-monitor.desktop"

CORE_INSTALLED=0
if [ -e "$BIN_DST" ] || [ -e "$SERVICE_DST" ] || [ -e "$AUTOSTART_DST" ]; then
    CORE_INSTALLED=1
fi

PLASMA_FILES_FOUND=0
if [ -d "$PLASMOID_DST" ] || [ -f "$ICON_DST" ]; then
    PLASMA_FILES_FOUND=1
fi

if [ "$CORE_INSTALLED" -eq 0 ]; then
    echo "dsmon executable and systemd service were not found. Core app is not installed."
    if [ "$PLASMA_FILES_FOUND" -eq 0 ]; then
        echo "Nothing to uninstall."
        exit 0
    fi
fi

if [ "$CORE_INSTALLED" -eq 1 ] && command -v systemctl >/dev/null 2>&1; then
    echo "Stopping and disabling $SERVICE_NAME..."
    systemctl --user disable --now "$SERVICE_NAME" >/dev/null 2>&1 || true
fi

if [ -e "$SERVICE_DST" ]; then
    rm -f "$SERVICE_DST"
    echo "Removed $SERVICE_DST"
else
    echo "Not found: $SERVICE_DST"
fi
if [ -e "$AUTOSTART_DST" ]; then
    rm -f "$AUTOSTART_DST"
    echo "Removed $AUTOSTART_DST"
else
    echo "Not found: $AUTOSTART_DST"
fi
if [ -e "$BIN_DST" ]; then
    rm -f "$BIN_DST"
    echo "Removed $BIN_DST"
else
    echo "Not found: $BIN_DST"
fi

if [ "$CORE_INSTALLED" -eq 1 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload || true
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
    echo "     rm -rf $PLASMOID_DST"
    echo "     rm -f $ICON_DST"
    echo "     gtk-update-icon-cache -q $ICON_CACHE_DIR 2>/dev/null || true"
else
    echo "No Plasma widget package files were detected."
fi

echo ""
echo "Older system-level files from previous versions are not removed by this"
echo "user-level uninstaller. Remove them manually with sudo if present:"
echo "  sudo rm -f /usr/local/bin/dsmon"
echo "  sudo rm -f /etc/systemd/user/dsmon.service"
echo "  sudo rm -rf /usr/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
echo "  sudo rm -f /usr/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"
