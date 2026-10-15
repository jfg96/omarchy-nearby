import QtQuick
import qs.Commons as Commons

// The shell's own catalog loader, plus the two I18n facts Nearby needs beside
// it. Strings.qml creates this on demand: an Omarchy without
// Commons.Translations cannot compile it, and Strings then stays in English.
Commons.Translations {
  readonly property bool rightToLeft: Commons.I18n.rightToLeft

  function formatNumber(value, decimals) {
    return Commons.I18n.formatNumber(value, decimals)
  }
}
