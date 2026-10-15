import QtQuick
import Quickshell.Io

// Nearby's user-facing text, from the catalogs in i18n/ (one flat JSON file
// per Omarchy language code).
//
// On an Omarchy that ships Commons.Translations, the shell's loader follows
// the language picked under Setup > Language, live, with its fallback chain
// and plural rules. An Omarchy without it cannot compile OmarchyStrings.qml,
// so this reads i18n/en.json itself and stays in English: the import must not
// keep the plugin from loading on a shell that predates translations.
//
// t() and tn() substitute %1, %2, ... in one pass. QML's chained .arg() would
// expand a placeholder that a sender's name or a filename happens to contain.
QtObject {
  id: root

  readonly property url directory: Qt.resolvedUrl("i18n")
  readonly property string directoryPath: String(directory).replace(/^file:\/\//, "")

  property QtObject shellStrings: null
  readonly property var englishStrings: {
    try {
      var parsed = JSON.parse(englishFile.text() || "{}")
      return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : ({})
    } catch (e) { return ({}) }
  }

  readonly property bool rightToLeft: shellStrings ? shellStrings.rightToLeft : false

  Component.onCompleted: {
    var component = Qt.createComponent(Qt.resolvedUrl("OmarchyStrings.qml"))
    if (component.status === Component.Ready) shellStrings = component.createObject(root, { directory: root.directory })
  }

  function format(text, args) {
    if (!args || args.length === 0) return text
    return String(text).replace(/%(\d+)/g, function(match, index) {
      var value = args[Number(index) - 1]
      return value === undefined ? match : String(value)
    })
  }

  function englishValue(key, count) {
    var value = root.englishStrings[key]
    if (value && typeof value === "object") value = count === 1 && value.one ? value.one : value.other
    return typeof value === "string" && value.length > 0 ? value : key
  }

  function t(key, args) {
    var fallback = englishValue(key)
    return format(shellStrings ? shellStrings.t(key, fallback) : fallback, args)
  }

  // A counted string. The count is not substituted on its own: pass it, through
  // number(), wherever the entry's placeholders want it.
  function tn(key, count, args) {
    var fallback = englishValue(key, count)
    return format(shellStrings ? shellStrings.tn(key, count, fallback) : fallback, args)
  }

  function number(value, decimals) {
    if (shellStrings) return shellStrings.formatNumber(value, decimals)
    return decimals === undefined ? String(value) : Number(value).toFixed(decimals)
  }

  property FileView englishFile: FileView {
    path: root.directoryPath + "/en.json"
    watchChanges: true
    printErrors: false
    blockLoading: true
    onFileChanged: reload()
  }
}
