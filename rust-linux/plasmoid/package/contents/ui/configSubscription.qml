import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM
import org.kde.plasma.plasma5support as Plasma5Support

KCM.SimpleKCM {
    id: page

    property bool busy: false
    property string statusText: ""
    property string ogStatusText: ""
    property string ccStatusText: ""
    property real ogRollingPercent: 0
    property real ogWeeklyPercent: 0
    property real ogMonthlyPercent: 0
    property string ogRollingText: "--"
    property string ogWeeklyText: "--"
    property string ogMonthlyText: "--"
    property real cc5hPercent: 0
    property real ccWeeklyPercent: 0
    property real ccMonthlyPercent: 0
    property string cc5hText: "--"
    property string ccWeeklyText: "--"
    property string ccMonthlyText: "--"
    property var loadCommands: []
    property bool loadingBatch: false
    property string pageLanguage: systemLanguage()
    property string cfg_language: pageLanguage
    property string cfg_languageDefault: systemLanguage()
    property bool cfg_expanding: false
    property int cfg_length: 0
    readonly property string uiLanguage: pageLanguage

    function systemLanguage() {
        var localeName = Qt.locale().name
        if (!localeName || String(localeName).length === 0) {
            return "zh"
        }
        return String(localeName).indexOf("zh") === 0 ? "zh" : "en"
    }

    function tr(key) {
        var zh = {
            groupOg: "OpenCode Go",
            groupCc: "Command Code",
            refresh: "刷新",
            og5h: "5h",
            ogWeekly: "每周",
            ogMonthly: "每月",
            ogUnavailable: "不可用",
            ogChecking: "查询 OpenCode Go 额度中...",
            ogError: "OpenCode Go 额度查询失败：",
            ogNotConfigured: "尚未配置 OpenCode Go API Key。请在「账户」页面输入并保存。",
            cc5h: "5h",
            ccWeekly: "每周",
            ccMonthly: "每月",
            ccUnavailable: "不可用",
            ccChecking: "查询 Command Code 额度中...",
            ccError: "Command Code 额度查询失败：",
            ccNotConfigured: "尚未配置 Command Code API Key。请在「账户」页面输入并保存。",
            loaded: "已加载。",
            loadFailed: "加载失败：",
            loading: "正在加载..."
        }
        var en = {
            groupOg: "OpenCode Go",
            groupCc: "Command Code",
            refresh: "Refresh",
            og5h: "5h",
            ogWeekly: "Weekly",
            ogMonthly: "Monthly",
            ogUnavailable: "unavailable",
            ogChecking: "Checking OpenCode Go quota...",
            ogError: "Failed to fetch OpenCode Go quota: ",
            ogNotConfigured: "OpenCode Go API key is not configured. Enter it on the Account page and save.",
            cc5h: "5h",
            ccWeekly: "Weekly",
            ccMonthly: "Monthly",
            ccUnavailable: "unavailable",
            ccChecking: "Checking Command Code quota...",
            ccError: "Failed to fetch Command Code quota: ",
            ccNotConfigured: "Command Code API key is not configured. Enter it on the Account page and save.",
            loaded: "Loaded.",
            loadFailed: "Failed to load: ",
            loading: "Loading..."
        }
        var table = uiLanguage === "zh" ? zh : en
        return table[key] || key
    }

    function barColor(percent) {
        if (percent >= 80) {
            return "#e53935"
        }
        if (percent >= 60) {
            return "#ffb300"
        }
        return "#4caf50"
    }

    function formatOgValue(usage) {
        if (!usage) {
            return tr("ogUnavailable")
        }
        return Math.round(usage.usage_percent) + "% · " + formatResetSeconds(usage.reset_in_sec)
    }

    function formatCcValue(window) {
        if (!window) {
            return tr("ccUnavailable")
        }
        return Number(window.used).toFixed(1) + "/" + Number(window.cap).toFixed(1) + " · "
            + formatResetSeconds(window.reset_in_sec)
    }

    function formatResetSeconds(secs) {
        if (secs <= 0) {
            return "now"
        }
        var days = Math.floor(secs / 86400)
        var hours = Math.floor((secs % 86400) / 3600)
        var minutes = Math.floor((secs % 3600) / 60)
        var parts = []
        if (days > 0) {
            parts.push(days + "d")
        }
        if (hours > 0) {
            parts.push(hours + "h")
        }
        if (minutes > 0) {
            parts.push(minutes + "m")
        }
        if (parts.length === 0) {
            parts.push((secs % 60) + "s")
        }
        return parts.join(" ")
    }

    function loadConfig() {
        busy = true
        statusText = tr("loading")
        loader.connectSource("dsmon config-json")
    }

    function refresh() {
        ogStatusText = tr("ogChecking")
        ccStatusText = tr("ccChecking")
        ogRollingPercent = 0
        ogWeeklyPercent = 0
        ogMonthlyPercent = 0
        ogRollingText = "--"
        ogWeeklyText = "--"
        ogMonthlyText = "--"
        cc5hPercent = 0
        ccWeeklyPercent = 0
        ccMonthlyPercent = 0
        cc5hText = "--"
        ccWeeklyText = "--"
        ccMonthlyText = "--"
        loader.connectSource("dsmon opencode-go json")
        loader.connectSource("dsmon command-code json")
    }

    Component.onCompleted: loadConfig()

    Plasma5Support.DataSource {
        id: loader
        engine: "executable"
        connectedSources: []
        onNewData: function(sourceName, data) {
            var stdout = data["stdout"] || ""
            var stderr = data["stderr"] || ""
            if (String(sourceName).indexOf("config-json") !== -1) {
                try {
                    var config = JSON.parse(stdout)
                    pageLanguage = config.ui_language === "zh" || config.ui_language === "en"
                        ? config.ui_language
                        : systemLanguage()
                } catch (error) {
                    pageLanguage = systemLanguage()
                }
                disconnectSource(sourceName)
                refresh()
                return
            }
            var isOg = String(sourceName).indexOf("opencode-go json") !== -1
            if (isOg) {
                busy = false
                if (stderr.trim().length > 0 && stdout.trim().length === 0) {
                    ogStatusText = tr("ogError") + stderr.trim()
                } else {
                    try {
                        var status = JSON.parse(stdout)
                        if (!status.configured) {
                            ogStatusText = tr("ogNotConfigured")
                        } else if (status.error && status.error.length > 0) {
                            ogStatusText = tr("ogError") + status.error
                        } else {
                            ogRollingPercent = status.rolling ? status.rolling.usage_percent : 0
                            ogWeeklyPercent = status.weekly ? status.weekly.usage_percent : 0
                            ogMonthlyPercent = status.monthly ? status.monthly.usage_percent : 0
                            ogRollingText = formatOgValue(status.rolling)
                            ogWeeklyText = formatOgValue(status.weekly)
                            ogMonthlyText = formatOgValue(status.monthly)
                            ogStatusText = ""
                        }
                    } catch (error) {
                        ogStatusText = tr("ogError") + error
                    }
                }
                disconnectSource(sourceName)
                return
            }
            if (String(sourceName).indexOf("command-code json") !== -1) {
                busy = false
                if (stderr.trim().length > 0 && stdout.trim().length === 0) {
                    ccStatusText = tr("ccError") + stderr.trim()
                } else {
                    try {
                        var cc = JSON.parse(stdout)
                        if (!cc.configured) {
                            ccStatusText = tr("ccNotConfigured")
                        } else if (cc.error && cc.error.length > 0) {
                            ccStatusText = tr("ccError") + cc.error
                        } else {
                            cc5hPercent = cc.five_hour && cc.five_hour.cap > 0
                                ? Math.min(100, Number(cc.five_hour.used) / Number(cc.five_hour.cap) * 100)
                                : 0
                            ccWeeklyPercent = cc.weekly && cc.weekly.cap > 0
                                ? Math.min(100, Number(cc.weekly.used) / Number(cc.weekly.cap) * 100)
                                : 0
                            ccMonthlyPercent = cc.monthly && cc.monthly.cap > 0
                                ? Math.min(100, Number(cc.monthly.used) / Number(cc.monthly.cap) * 100)
                                : 0
                            cc5hText = formatCcValue(cc.five_hour)
                            ccWeeklyText = formatCcValue(cc.weekly)
                            ccMonthlyText = formatCcValue(cc.monthly)
                            ccStatusText = ""
                        }
                    } catch (error) {
                        ccStatusText = tr("ccError") + error
                    }
                }
                disconnectSource(sourceName)
                return
            }
            busy = false
            disconnectSource(sourceName)
        }
    }

    Kirigami.FormLayout {
        QtControls.Label {
            text: tr("groupOg")
            font.bold: true
            Layout.fillWidth: true
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Kirigami.Theme.disabledTextColor
        }

        QtControls.Button {
            text: tr("refresh")
            enabled: !busy
            onClicked: refresh()
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("og5h")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: ogRollingBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.ogRollingPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: ogRollingBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(ogRollingBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.ogRollingText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("ogWeekly")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: ogWeeklyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.ogWeeklyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: ogWeeklyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(ogWeeklyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.ogWeeklyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("ogMonthly")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: ogMonthlyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.ogMonthlyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: ogMonthlyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(ogMonthlyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.ogMonthlyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        QtControls.Label {
            Layout.fillWidth: true
            visible: ogStatusText.length > 0
            text: ogStatusText
            wrapMode: Text.WordWrap
        }

        QtControls.Label {
            text: tr("groupCc")
            font.bold: true
            Layout.fillWidth: true
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Kirigami.Theme.disabledTextColor
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("cc5h")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: cc5hBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.cc5hPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: cc5hBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(cc5hBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.cc5hText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("ccWeekly")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: ccWeeklyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.ccWeeklyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: ccWeeklyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(ccWeeklyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.ccWeeklyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("ccMonthly")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: ccMonthlyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.ccMonthlyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: ccMonthlyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(ccMonthlyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.ccMonthlyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                horizontalAlignment: Text.AlignRight
            }
        }

        QtControls.Label {
            Layout.fillWidth: true
            visible: ccStatusText.length > 0
            text: ccStatusText
            wrapMode: Text.WordWrap
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: statusText
            wrapMode: Text.WordWrap
        }
    }
}
