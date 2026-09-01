import org.kde.plasma.configuration

ConfigModel {
    function systemLanguage() {
        var localeName = Qt.locale().name
        if (!localeName || String(localeName).length === 0) {
            return "zh"
        }
        return String(localeName).indexOf("zh") === 0 ? "zh" : "en"
    }

    function tr(key) {
        var zh = { account: "账户", general: "常规", history: "历史", subscription: "订阅" }
        var en = { account: "Account", general: "General", history: "History", subscription: "Subscriptions" }
        var table = systemLanguage() === "zh" ? zh : en
        return table[key] || key
    }

    ConfigCategory {
        name: tr("account")
        icon: "preferences-system-users"
        source: "configAccount.qml"
    }
    ConfigCategory {
        name: tr("subscription")
        icon: "preferences-system-network"
        source: "configSubscription.qml"
    }
    ConfigCategory {
        name: tr("general")
        icon: "configure"
        source: "configGeneral.qml"
    }
    ConfigCategory {
        name: tr("history")
        icon: "view-history"
        source: "configHistory.qml"
    }
}
