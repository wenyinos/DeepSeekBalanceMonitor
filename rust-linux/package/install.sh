#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN_SRC="$SCRIPT_DIR/dsmon"
SERVICE_SRC="$SCRIPT_DIR/dsmon.service"
PLASMOID_SRC="$SCRIPT_DIR/plasmoid"
SERVICE_NAME="dsmon.service"

# 纯用户级安装：不写任何系统目录，无需 sudo
INSTALL_USER="$(id -un)"
USER_HOME="${HOME:-}"
if [ -z "$USER_HOME" ]; then
    USER_HOME="$(getent passwd "$INSTALL_USER" | cut -d: -f6)"
fi
if [ -z "$USER_HOME" ]; then
    echo "Unable to determine home directory for user $INSTALL_USER." >&2
    exit 1
fi

BIN_DST="$USER_HOME/.local/bin/dsmon"
SERVICE_DST="$USER_HOME/.config/systemd/user/dsmon.service"
PLASMOID_DST="$USER_HOME/.local/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
ICON_DST="$USER_HOME/.local/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"
ICON_CACHE_DIR="$USER_HOME/.local/share/icons/hicolor"

# 旧版本安装到系统目录的残留（检测并提示，不自动删除）
OLD_SYSTEM_BIN="/usr/local/bin/dsmon"
OLD_SYSTEM_SERVICE="/etc/systemd/user/dsmon.service"
OLD_SYSTEM_PLASMOID="/usr/share/plasma/plasmoids/com.github.wenyinos.deepseek-balance-monitor"
OLD_SYSTEM_ICON="/usr/share/icons/hicolor/256x256/apps/deepseek-balance-monitor.png"

echo "This installer installs everything under your user directory ($USER_HOME)."
echo "No sudo is required."

if [ "$(id -u)" -eq 0 ]; then
    echo ""
    echo "WARNING: you are running this installer as root (e.g. via sudo)."
    echo "This installer is user-level only and should be run as your normal user."
    printf "Continue as root anyway? [y/N] "
    IFS= read -r answer || answer=""
    case "$answer" in
        y|Y|yes|YES|Yes) ;;
        *) echo "Aborted. Run without sudo: ./install.sh"; exit 1 ;;
    esac
fi

# 检测旧系统级残留：如存在则询问是否用 sudo 自动删除（可能要求输入密码）
OLD_SYSTEM_FOUND=0
[ -e "$OLD_SYSTEM_BIN" ] && OLD_SYSTEM_FOUND=1
[ -e "$OLD_SYSTEM_SERVICE" ] && OLD_SYSTEM_FOUND=1
[ -d "$OLD_SYSTEM_PLASMOID" ] && OLD_SYSTEM_FOUND=1
[ -f "$OLD_SYSTEM_ICON" ] && OLD_SYSTEM_FOUND=1
if [ "$OLD_SYSTEM_FOUND" -eq 1 ]; then
    echo ""
    echo "NOTE: older system-level files from a previous version were detected:"
    [ -e "$OLD_SYSTEM_BIN" ] && echo "  - $OLD_SYSTEM_BIN"
    [ -e "$OLD_SYSTEM_SERVICE" ] && echo "  - $OLD_SYSTEM_SERVICE"
    [ -d "$OLD_SYSTEM_PLASMOID" ] && echo "  - $OLD_SYSTEM_PLASMOID"
    [ -f "$OLD_SYSTEM_ICON" ] && echo "  - $OLD_SYSTEM_ICON"
    echo "They can shadow or conflict with the user-level install."
    printf "Remove them now with sudo (may ask for your password)? [y/N] "
    IFS= read -r answer || answer=""
    case "$answer" in
        y|Y|yes|YES|Yes)
            if command -v sudo >/dev/null 2>&1; then
                echo "Removing old system-level files with sudo..."
                [ -e "$OLD_SYSTEM_BIN" ] && sudo rm -f "$OLD_SYSTEM_BIN" || true
                [ -e "$OLD_SYSTEM_SERVICE" ] && sudo rm -f "$OLD_SYSTEM_SERVICE" || true
                [ -d "$OLD_SYSTEM_PLASMOID" ] && sudo rm -rf "$OLD_SYSTEM_PLASMOID" || true
                [ -f "$OLD_SYSTEM_ICON" ] && sudo rm -f "$OLD_SYSTEM_ICON" || true
                echo "Old system-level files removed."
            else
                echo "sudo is not available. Remove them manually:"
                [ -e "$OLD_SYSTEM_BIN" ] && echo "  sudo rm -f $OLD_SYSTEM_BIN"
                [ -e "$OLD_SYSTEM_SERVICE" ] && echo "  sudo rm -f $OLD_SYSTEM_SERVICE"
                [ -d "$OLD_SYSTEM_PLASMOID" ] && echo "  sudo rm -rf $OLD_SYSTEM_PLASMOID"
                [ -f "$OLD_SYSTEM_ICON" ] && echo "  sudo rm -f $OLD_SYSTEM_ICON"
            fi
            ;;
        *)
            echo "Skipped removal. They will not interfere with the user-level install,"
            echo "but may shadow it. Remove them later with:"
            echo "  sudo rm -f $OLD_SYSTEM_BIN"
            echo "  sudo rm -f $OLD_SYSTEM_SERVICE"
            echo "  sudo rm -rf $OLD_SYSTEM_PLASMOID"
            echo "  sudo rm -f $OLD_SYSTEM_ICON"
            ;;
    esac
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

# 安装二进制
install -d -m 755 "$USER_HOME/.local/bin"
install -m 755 "$BIN_SRC" "$BIN_DST"

# 检测 init 系统：有 systemctl 则用 systemd 用户服务，否则用桌面 autostart
if command -v systemctl >/dev/null 2>&1; then
    USE_SYSTEMD=1
else
    USE_SYSTEMD=0
fi

if [ "$USE_SYSTEMD" -eq 1 ]; then
    # 安装 systemd 用户服务（ExecStart 指向用户级二进制）
    install -d -m 755 "$USER_HOME/.config/systemd/user"
    sed -e "s|/usr/local/bin/dsmon|%h/.local/bin/dsmon|g" "$SERVICE_SRC" > "$SERVICE_DST"
    chmod 644 "$SERVICE_DST"
else
    # 非 systemd 发行版：不安装 service 文件，写入桌面 autostart 条目
    AUTOSTART_DST="$USER_HOME/.config/autostart/deepseek-balance-monitor.desktop"
    install -d -m 755 "$(dirname "$AUTOSTART_DST")"
    cat > "$AUTOSTART_DST" <<EOF
[Desktop Entry]
Type=Application
Name=DeepSeek Balance Monitor daemon
Comment=Start the dsmon daemon at login
Exec=$BIN_DST daemon
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
    echo "Wrote desktop autostart entry: $AUTOSTART_DST"
fi

if [ "$INSTALL_PLASMOID" -eq 1 ]; then
    # 安装 plasmoid（用户级）
    install -d -m 755 "$(dirname "$PLASMOID_DST")"
    rm -rf "$PLASMOID_DST"
    cp -R "$PLASMOID_SRC" "$PLASMOID_DST"

    # 安装图标（用户级）
    install -d -m 755 "$(dirname "$ICON_DST")"
    install -m 644 "$PLASMOID_SRC/contents/images/deepseek-balance-monitor.png" "$ICON_DST"

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

# 确保 ~/.local/bin 在 PATH：不在时自动写入 shell 配置（带标记，可重复运行）
PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
PATH_MARKER="# >>> deepseek-balance-monitor: ~/.local/bin PATH >>>"
if ! case ":$PATH:" in *":$USER_HOME/.local/bin:"*) ;; *) false ;; esac; then
    for PROFILE in "$USER_HOME/.profile" "$USER_HOME/.bashrc"; do
        if [ -f "$PROFILE" ] || { [ "$PROFILE" = "$USER_HOME/.profile" ] && [ ! -e "$USER_HOME/.bashrc" ]; }; then
            touch "$PROFILE" 2>/dev/null || continue
            if ! grep -qF "$PATH_MARKER" "$PROFILE" 2>/dev/null; then
                printf '\n%s\n%s\n%s\n' "$PATH_MARKER" "$PATH_LINE" "# <<< deepseek-balance-monitor >>>" >> "$PROFILE"
                echo "Added $PATH_LINE to $PROFILE"
            fi
        fi
    done
    echo ""
    echo "NOTE: $USER_HOME/.local/bin was not on your PATH; it has been added to your shell profile."
    echo "Open a new terminal or run:  source ~/.profile  (or  source ~/.bashrc)"
    echo ""
fi

# 自动设为自启动：systemd 下 enable --now；非 systemd 下已写桌面 autostart
if [ "$USE_SYSTEMD" -eq 1 ]; then
    echo "Reloading user systemd manager..."
    systemctl --user daemon-reload || true
    echo "Enabling and starting $SERVICE_NAME..."
    if systemctl --user enable --now "$SERVICE_NAME" 2>/dev/null; then
        echo "$SERVICE_NAME enabled (auto-start on login) and started."
    else
        echo "Failed to enable/start $SERVICE_NAME automatically."
        echo "Do it manually with: systemctl --user enable --now $SERVICE_NAME"
    fi
else
    echo "No systemd detected. The daemon will auto-start at desktop login"
    echo "via $USER_HOME/.config/autostart/deepseek-balance-monitor.desktop."
    echo "For non-desktop sessions, start it manually with: $BIN_DST daemon"
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
    if printf "%s\n" "$API_KEY" | "$BIN_DST" set-key; then
        echo "Running check after saving API key..."
        if ! "$BIN_DST" check; then
            echo "API key was saved, but the check still failed. Please review the output above."
        fi
    else
        echo "Failed to save API key. Set it later with: dsmon set-key <api_key>" >&2
    fi
}
echo "Running first check..."
CHECK_STATUS=0
CHECK_OUTPUT="$("$BIN_DST" check 2>&1)" || CHECK_STATUS=$?
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
if [ "$USE_SYSTEMD" -eq 1 ]; then
    echo "Installation complete. $SERVICE_NAME is set to auto-start on login."
    echo "If it is not running, start it now with: systemctl --user start dsmon.service"
else
    echo "Installation complete. The dsmon daemon will auto-start at desktop login"
    echo "via the autostart entry; for non-desktop sessions run: $BIN_DST daemon"
fi
if [ "$INSTALL_PLASMOID" -eq 1 ]; then
    echo "Add widget: right-click panel/desktop -> Add Widgets -> DeepSeek Balance Monitor"
    echo "If the old Plasma widget UI or icon is still shown, restart plasmashell or log out and log back in."
fi
