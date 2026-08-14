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
    property string opencodeGoText: ""
    property bool hasOgApiKey: false
    property string loadedOgApiKey: ""
    property real rollingPercent: 0
    property real weeklyPercent: 0
    property real monthlyPercent: 0
    property string rollingText: "--"
    property string weeklyText: "--"
    property string monthlyText: "--"
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
            opencodeGoTitle: "OpenCode Go 额度",
            ogNotConfigured: "未配置 OpenCode Go API Key。请在终端运行 dsmon opencode-go set-key <api_key> 进行配置。",
            ogChecking: "查询 OpenCode Go 额度中...",
            ogError: "OpenCode Go 额度查询失败：",
            og5h: "5h",
            ogWeekly: "每周",
            ogMonthly: "每月",
            ogUnavailable: "不可用",
            apiKey: "OpenCode Go API Key：",
            apiKeyStored: "********",
            save: "保存",
            saving: "正在保存...",
            saved: "已保存。",
            saveFailed: "保存失败：",
            ogSetKeyPlaceholder: "dsmon opencode-go set-key",
            loading: "正在加载...",
            loaded: "已加载。",
            loadFailed: "加载失败：",
            refresh: "刷新"
        }
        var en = {
            opencodeGoTitle: "OpenCode Go Quota",
            ogNotConfigured: "OpenCode Go API key is not configured. Run dsmon opencode-go set-key <api_key> in a terminal.",
            ogChecking: "Checking OpenCode Go quota...",
            ogError: "Failed to fetch OpenCode Go quota: ",
            og5h: "5h",
            ogWeekly: "Weekly",
            ogMonthly: "Monthly",
            ogUnavailable: "unavailable",
            apiKey: "OpenCode Go API key:",
            apiKeyStored: "********",
            save: "Save",
            saving: "Saving...",
            saved: "Saved.",
            saveFailed: "Failed to save: ",
            ogSetKeyPlaceholder: "dsmon opencode-go set-key",
            loading: "Loading...",
            loaded: "Loaded.",
            loadFailed: "Failed to load: ",
            refresh: "Refresh"
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
        loader.connectSource("/usr/local/bin/dsmon config-json")
    }

    function loadOpencodeGo() {
        busy = true
        statusText = tr("loading")
        opencodeGoText = tr("ogChecking")
        rollingPercent = 0
        weeklyPercent = 0
        monthlyPercent = 0
        rollingText = "--"
        weeklyText = "--"
        monthlyText = "--"
        loader.connectSource("/usr/local/bin/dsmon opencode-go json")
    }

    function refresh() {
        loadOpencodeGo()
    }

    function shellQuote(value) {
        return "'" + String(value).replace(/'/g, "'\\''") + "'"
    }

    function saveApiKey() {
        var value = ogApiKeyField.text.trim()
        if (value.length === 0 || value === tr("apiKeyStored")) {
            return
        }
        busy = true
        statusText = tr("saving")
        loader.connectSource("/usr/local/bin/dsmon opencode-go set-key " + shellQuote(value))
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
                loadOpencodeGo()
                return
            }
            if (String(sourceName).indexOf("opencode-go set-key") !== -1) {
                busy = false
                if (stderr.trim().length > 0) {
                    statusText = tr("saveFailed") + stderr.trim()
                } else {
                    statusText = tr("saved")
                    loadOpencodeGo()
                }
                disconnectSource(sourceName)
                return
            }
            busy = false
            if (stderr.trim().length > 0 && stdout.trim().length === 0) {
                statusText = tr("ogError") + stderr.trim()
            } else {
                try {
                    var status = JSON.parse(stdout)
                    if (!status.configured) {
                        hasOgApiKey = false
                        ogApiKeyField.text = ""
                        ogApiKeyField.placeholderText = tr("ogSetKeyPlaceholder")
                        opencodeGoText = tr("ogNotConfigured")
                    } else {
                        hasOgApiKey = true
                        loadedOgApiKey = tr("apiKeyStored")
                        ogApiKeyField.text = loadedOgApiKey
                        ogApiKeyField.placeholderText = tr("apiKeyStored")
                        if (status.error && status.error.length > 0) {
                            opencodeGoText = tr("ogError") + status.error
                        } else {
                            rollingPercent = status.rolling ? status.rolling.usage_percent : 0
                            weeklyPercent = status.weekly ? status.weekly.usage_percent : 0
                            monthlyPercent = status.monthly ? status.monthly.usage_percent : 0
                            rollingText = formatOgValue(status.rolling)
                            weeklyText = formatOgValue(status.weekly)
                            monthlyText = formatOgValue(status.monthly)
                            opencodeGoText = tr("opencodeGoTitle")
                        }
                    }
                    statusText = tr("loaded")
                } catch (error) {
                    opencodeGoText = tr("ogError") + error
                    statusText = tr("loadFailed") + error
                }
            }
            disconnectSource(sourceName)
        }
    }

    Kirigami.FormLayout {
        QtControls.TextField {
            id: ogApiKeyField
            Kirigami.FormData.label: tr("apiKey")
            Layout.fillWidth: true
            echoMode: TextInput.Password
        }

        RowLayout {
            Layout.fillWidth: true

            QtControls.Button {
                text: tr("save")
                enabled: !busy
                onClicked: saveApiKey()
            }
            QtControls.Button {
                text: tr("refresh")
                enabled: !busy
                onClicked: refresh()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QtControls.Label {
                text: tr("og5h")
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
            }
            QtControls.ProgressBar {
                id: rollingBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.rollingPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: rollingBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(rollingBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.rollingText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
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
                id: weeklyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.weeklyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: weeklyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(weeklyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.weeklyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
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
                id: monthlyBar
                Layout.fillWidth: true
                Layout.preferredHeight: 14
                from: 0
                to: 100
                value: page.monthlyPercent
                background: Rectangle {
                    radius: 7
                    color: Kirigami.Theme.backgroundColor
                    border.color: Kirigami.Theme.disabledTextColor
                    border.width: 1
                }
                contentItem: Item {
                    Rectangle {
                        width: monthlyBar.visualPosition * parent.width
                        height: parent.height
                        radius: 7
                        color: page.barColor(monthlyBar.value)
                    }
                }
            }
            QtControls.Label {
                text: page.monthlyText
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                horizontalAlignment: Text.AlignRight
            }
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: opencodeGoText
            wrapMode: Text.WordWrap
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: statusText
            wrapMode: Text.WordWrap
        }
    }
}
