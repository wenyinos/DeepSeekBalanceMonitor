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
    property bool hasStoredApiKey: false
    property string loadedApiKey: ""
    property bool hasOgApiKey: false
    property string loadedOgApiKey: ""
    property bool hasCcApiKey: false
    property string loadedCcApiKey: ""
    property var saveCommands: []
    property bool savingBatch: false
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
            groupCredentials: "凭据",
            apiKey: "DeepSeek API Key：",
            apiKeyStored: "********",
            apiKeyUpdateHint: "API Key 已加密保存。修改真实 Key 时请在终端运行 dsmon set-key；如需演示模式，可直接输入 demo 后保存。",
            showApiKey: "显示 API Key",
            ogApiKeyLabel: "OpenCode Go API Key：",
            ogApiKeyHint: "留空则保留现有 API Key。",
            ogHint: "API Key 可在 https://opencode.ai/auth 获取并填入上方。密钥将加密存储在本机。",
            ccApiKeyLabel: "Command Code API Key：",
            ccApiKeyHint: "留空则保留现有 API Key。",
            ccHint: "API Key 可在 https://commandcode.ai 获取并填入上方。密钥将加密存储在本机。",
            save: "保存凭据",
            saving: "正在保存...",
            saved: "已保存。",
            saveFailed: "保存失败：",
            loading: "正在加载...",
            loaded: "已加载。"
        }
        var en = {
            groupCredentials: "Credentials",
            apiKey: "DeepSeek API key:",
            apiKeyStored: "********",
            apiKeyUpdateHint: "API key is stored encrypted. To update a real key, run dsmon set-key in a terminal. For demo mode, enter demo here and save.",
            showApiKey: "Show API key",
            ogApiKeyLabel: "OpenCode Go API key:",
            ogApiKeyHint: "Leave blank to keep the existing API key.",
            ogHint: "Get an API key from https://opencode.ai/auth and enter it above. The key is encrypted and stored locally.",
            ccApiKeyLabel: "Command Code API key:",
            ccApiKeyHint: "Leave blank to keep the existing API key.",
            ccHint: "Get an API key from https://commandcode.ai and enter it above. The key is encrypted and stored locally.",
            save: "Save credentials",
            saving: "Saving...",
            saved: "Saved.",
            saveFailed: "Failed to save: ",
            loading: "Loading...",
            loaded: "Loaded."
        }
        var table = uiLanguage === "zh" ? zh : en
        return table[key] || key
    }

    function shellQuote(value) {
        return "'" + String(value).replace(/'/g, "'\\''") + "'"
    }

    function loadConfig() {
        busy = true
        statusText = tr("loading")
        loader.connectSource("dsmon config-json")
    }

    function runNextSaveCommand() {
        if (saveCommands.length === 0) {
            savingBatch = false
            busy = false
            statusText = tr("saved")
            loadConfig()
            return
        }
        loader.connectSource(saveCommands.shift())
    }

    function saveKeys() {
        var commands = []
        var dsKey = apiKeyField.text.trim()
        if (dsKey.length > 0 && dsKey !== tr("apiKeyStored")) {
            if (dsKey.toLowerCase() === "demo") {
                commands.push("dsmon set-key " + shellQuote("demo"))
            } else {
                statusText = tr("apiKeyUpdateHint")
                busy = false
                return
            }
        }
        var ogKey = ogApiKeyField.text.trim()
        if (ogKey.length > 0 && ogKey !== tr("apiKeyStored")) {
            commands.push("dsmon opencode-go set-key " + shellQuote(ogKey))
        }
        var ccKey = ccApiKeyField.text.trim()
        if (ccKey.length > 0 && ccKey !== tr("apiKeyStored")) {
            commands.push("dsmon command-code set-key " + shellQuote(ccKey))
        }
        if (commands.length === 0) {
            return
        }
        busy = true
        statusText = tr("saving")
        saveCommands = commands
        savingBatch = true
        runNextSaveCommand()
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
                    hasStoredApiKey = !!config.has_key || (config.api_key || "").length > 0
                        || config.api_key === "masked"
                    loadedApiKey = hasStoredApiKey ? tr("apiKeyStored") : ""
                    apiKeyField.text = loadedApiKey
                    apiKeyField.placeholderText = hasStoredApiKey ? tr("apiKeyStored") : "dsmon set-key"
                } catch (error) {
                    pageLanguage = systemLanguage()
                }
                disconnectSource(sourceName)
                return
            }
            if (String(sourceName).indexOf("set-key") !== -1) {
                if (savingBatch) {
                    if (stderr.trim().length > 0) {
                        saveCommands = []
                        savingBatch = false
                        busy = false
                        statusText = tr("saveFailed") + stderr.trim()
                    } else {
                        runNextSaveCommand()
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
            text: tr("groupCredentials")
            font.bold: true
            Layout.fillWidth: true
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Kirigami.Theme.disabledTextColor
        }

        QtControls.TextField {
            id: apiKeyField
            Kirigami.FormData.label: tr("apiKey")
            Layout.fillWidth: true
            echoMode: TextInput.Password
        }

        QtControls.CheckBox {
            id: showDsKeyCheck
            text: tr("showApiKey")
            onToggled: apiKeyField.echoMode = checked ? TextInput.Normal : TextInput.Password
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: tr("apiKeyUpdateHint")
            wrapMode: Text.WordWrap
        }

        QtControls.TextField {
            id: ogApiKeyField
            Kirigami.FormData.label: tr("ogApiKeyLabel")
            Layout.fillWidth: true
            echoMode: TextInput.Password
        }

        QtControls.CheckBox {
            id: showOgKeyCheck
            text: tr("showApiKey")
            onToggled: ogApiKeyField.echoMode = checked ? TextInput.Normal : TextInput.Password
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: tr("ogHint")
            wrapMode: Text.WordWrap
        }

        QtControls.TextField {
            id: ccApiKeyField
            Kirigami.FormData.label: tr("ccApiKeyLabel")
            Layout.fillWidth: true
            echoMode: TextInput.Password
        }

        QtControls.CheckBox {
            id: showCcKeyCheck
            text: tr("showApiKey")
            onToggled: ccApiKeyField.echoMode = checked ? TextInput.Normal : TextInput.Password
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: tr("ccHint")
            wrapMode: Text.WordWrap
        }

        QtControls.Button {
            text: tr("save")
            enabled: !busy
            onClicked: saveKeys()
        }

        QtControls.Label {
            Layout.fillWidth: true
            text: statusText
            wrapMode: Text.WordWrap
        }
    }
}
