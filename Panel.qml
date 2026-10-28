import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons
import qs.Commons as Commons
import "Model.js" as Model

Panel {
  id: root
  moduleName: "oma.nearby"
  // The `oma.nearby` IPC target belongs to the service, which exists once. A
  // widget registering it would register it once per monitor.
  manageIpc: false

  Strings { id: strings }

  // The host may replace moduleName with an instance id. Keep the manifest id
  // stable for registry lookups and filesystem paths.
  readonly property string manifestPluginId: "oma.nearby"

  // The bar builds one of these per monitor, so this file owns no helper, no
  // transfer state, and no IPC target. All of that lives in the `service`
  // entry point, which the shell loads exactly once; everything below is a
  // view onto it plus this popup's own cursor. See Service.qml.
  readonly property var engine: bar && bar.shell && typeof bar.shell.serviceFor === "function"
    ? bar.shell.serviceFor(manifestPluginId)
    : null

  readonly property color foreground: bar ? bar.foreground : Commons.Color.foreground
  readonly property color urgent: bar ? bar.urgent : Commons.Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.4)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property color barIconColor: root.receiverEnabled ? barForeground : Qt.darker(barForeground, 1.55)

  // Engine state, read under the names the popup already used. A view with no
  // engine says so rather than rendering an empty panel.
  readonly property bool backendReady: engine ? engine.backendReady : false
  readonly property bool receiverEnabled: engine ? engine.receiverEnabled : false
  readonly property bool discoveryActive: engine ? engine.discoveryActive : false
  readonly property var devices: engine ? engine.devices : []
  readonly property var selectedDevice: engine ? engine.selectedDevice : null
  readonly property string viewState: engine ? engine.viewState : "nearby"
  readonly property string statusText: engine ? engine.statusText : strings.t("engine.notLoaded")
  readonly property string errorText: engine ? engine.errorText : strings.t("engine.notLoaded")
  readonly property var incomingQueue: engine ? engine.incomingQueue : []
  readonly property var incoming: engine ? engine.incoming : null
  readonly property string incomingMessage: root.incoming ? Model.incomingMessage(root.incoming.files) : ""
  readonly property string incomingText: engine ? engine.incomingText : ""
  readonly property real progress: engine ? engine.progress : 0
  readonly property string progressDetail: engine ? engine.progressDetail : ""
  readonly property string saveDirectory: engine ? engine.saveDirectory : ""
  readonly property string downloadDir: engine ? engine.downloadDir : ""
  readonly property string receivedFile: engine ? engine.receivedFile : ""
  readonly property bool receivedIsImage: engine ? Model.previewableImage(engine.receivedFileType) : false
  readonly property string receivedPreview: engine ? engine.receivedPreview : ""
  readonly property string transferName: engine ? engine.transferName : ""
  readonly property string transferPeer: engine ? engine.transferPeer : ""
  readonly property string pinError: engine ? engine.pinError : ""
  readonly property bool incomingPinEnabled: engine ? engine.incomingPinEnabled : false
  readonly property bool incomingPinUpdating: engine ? engine.incomingPinUpdating : false
  readonly property string incomingPinError: engine ? engine.incomingPinError : ""
  readonly property bool helperUpdateOffered: engine ? engine.helperUpdateOffered : false
  readonly property bool helperUpdating: engine ? engine.helperUpdating : false
  readonly property string helperUpdateStatus: engine ? engine.helperUpdateStatus : ""
  readonly property string helperUpdateError: engine ? engine.helperUpdateError : ""
  readonly property string helperUpdateDetail: engine ? engine.helperUpdateDetail : ""
  // An unusable helper takes the whole view over: there are no devices to list
  // and none are coming, so the section stops calling itself DEVICES.
  readonly property bool helperBlocksNearby: helperUpdateOffered && !backendReady

  // Shown only when helper recovery fails, so the user can refresh the tracked
  // plugin source through Omarchy or inspect its repository.
  readonly property string repositoryUrl: "https://github.com/jfg96/omarchy-nearby"
  readonly property string updateCommand: "omarchy plugin update oma.nearby"

  // A chosen folder can be given up for the Downloads lookup. The row follows
  // the folder row; while the receiver is not ready it follows the devices,
  // because a folder that has become unavailable keeps the receiver from
  // starting and hides the folder row with the rest of the actions.
  readonly property int useDownloadsIndex: !receiverEnabled || downloadDir === "" || (helperUpdateOffered && !backendReady)
    ? -1
    : (backendReady ? devices.length + 3 : devices.length)
  // The update row sits after the devices, the compact action row and the
  // folder rows. If updating fails, its two recovery actions follow it in
  // keyboard order.
  readonly property int helperUpdateIndex: !helperUpdateOffered
    ? -1
    : (useDownloadsIndex >= 0
      ? useDownloadsIndex + 1
      : (receiverEnabled && backendReady ? devices.length + 3 : devices.length))
  readonly property int helperCopyCommandIndex: helperUpdateError === "" || helperUpdateIndex < 0
    ? -1 : helperUpdateIndex + 1
  readonly property int helperCopyLinkIndex: helperCopyCommandIndex < 0
    ? -1 : helperCopyCommandIndex + 1

  // Cursor and popup focus are per monitor, so they stay here.
  property int selectedIndex: 0
  property bool cursorActive: false
  property int nearbyPhraseIndex: 0
  property bool viewRegistered: false
  property string copyNote: ""

  // None of these repeats the empty device list below, which already says
  // "Finding devices…" while discovery runs; the hero line would echo it.
  readonly property var nearbyPhrases: [
    "phrase.lookingNearby", "phrase.listeningLocally", "status.readyToReceive",
    "phrase.watchingLan", "phrase.checkingAir"
  ]
  readonly property bool rotatingPhrases: opened && receiverEnabled && backendReady && viewState === "nearby" && errorText === ""
  readonly property string heroMetaText: {
    if (viewState === "nearby") {
      if (!receiverEnabled) return strings.t("status.turnedOff")
      // statusText, never errorText: this line is uppercased and letterspaced,
      // so a full sentence crops mid-word and the body below is already saying
      // the same thing in full.
      if (!backendReady) return statusText
      return strings.t(nearbyPhrases[nearbyPhraseIndex % nearbyPhrases.length])
    }
    if (viewState === "incoming_pin_settings") return incomingPinEnabled ? strings.t("hero.protectionEnabled") : strings.t("hero.protectionDisabled")
    if (viewState === "incoming_pin_edit") return incomingPinEnabled ? strings.t("hero.changeProtection") : strings.t("hero.enableProtection")
    if (viewState === "incoming_pin_disable") return strings.t("hero.confirmDisable")
    if (viewState === "target") return strings.t("status.readyToReceive")
    return statusText
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Component.onCompleted: syncOpenState()

  // Discovery follows the popup and there is one popup per monitor, so the
  // engine counts open views instead of tracking a single flag.
  function syncOpenState() {
    if (!engine || opened === viewRegistered) return
    viewRegistered = opened
    if (opened) engine.viewOpened()
    else engine.viewClosed()
  }
  onEngineChanged: { viewRegistered = false; syncOpenState() }
  Component.onDestruction: if (engine && viewRegistered) engine.viewClosed()

  Connections {
    target: root.engine
    function onCursorRequested(index) { root.selectedIndex = index }
    function onPinCleared() { pinInput.text = "" }
    function onPinFocusRequested() { if (root.opened) Qt.callLater(function(){ pinInput.forceActiveFocus() }) }
    function onIncomingPinCleared() { incomingPinInput.text = "" }
    function onIncomingPinFocusRequested() { if (root.opened) Qt.callLater(function(){ incomingPinInput.forceActiveFocus() }) }
    function onFocusRestoreRequested() { if (root.opened) Qt.callLater(function(){ keyCatcher.forceActiveFocus() }) }
  }

  function toggleReceiver() { if (engine) engine.toggleReceiver() }
  function startDiscovery() { if (engine) engine.startDiscovery() }
  function forceFullDiscovery() { if (engine) engine.forceFullDiscovery() }
  function chooseDevice(index) { if (engine) engine.chooseDevice(index) }
  function acceptIncoming() { if (engine) engine.acceptIncoming() }
  function declineIncoming() { if (engine) engine.declineIncoming() }
  function finishText() { if (engine) engine.finishText() }
  function finishTerminal() { if (engine) engine.finishTerminal() }
  function cancelOutgoing() { if (engine) engine.cancelOutgoing() }
  function cancelPin() { if (engine) engine.cancelPin() }
  function retryWithPin() { if (engine) engine.retryWithPin(String(pinInput.text || "")) }
  function openIncomingPinSettings() { if (engine) engine.openIncomingPinSettings() }
  function beginIncomingPinEdit() { if (engine) engine.beginIncomingPinEdit() }
  function requestDisableIncomingPin() { if (engine) engine.requestDisableIncomingPin() }
  function cancelIncomingPinSettings() { if (engine) engine.cancelIncomingPinSettings() }
  function submitIncomingPin() { if (engine) engine.submitIncomingPin(String(incomingPinInput.text || "")) }
  function confirmDisableIncomingPin() { if (engine) engine.confirmDisableIncomingPin() }
  function clearSecretInputs() { pinInput.text = ""; incomingPinInput.text = "" }
  function retryHelper() { copyNote=""; if (engine) engine.startHelperUpdate() }
  function copyText(value) { if (textCopier.running) return; copyNote=""; textCopier.payload=String(value); textCopier.launched=false; textCopier.running=true }
  function copyUpdateCommand() { copyText(updateCommand) }
  function copyRepositoryLink() { copyText(repositoryUrl) }
  function failWith(message) { if (engine) engine.failWith(message) }
  function noteTextCopied() { if (engine) engine.noteTextCopied() }
  function beginOutgoing(pending) { if (engine) engine.beginOutgoing(pending) }
  function goBack() { if (viewState === "pin") cancelPin(); else if (viewState.indexOf("incoming_pin_")===0) cancelIncomingPinSettings(); else if (viewState === "target") { if (engine) engine.clearTarget() } else close() }

  // The chooser and the clipboard belong to the monitor the user acted on, so
  // they stay with the view and hand their result to the engine.
  function selectFiles() { if (!selectedDevice || picker.running) return; picker.launched=false; picker.running = true }
  function selectFolder() { if (!selectedDevice || folderPicker.running) return; folderPicker.launched=false; folderPicker.running = true }
  function chooseDownloadDir() { if (downloadDirPicker.running) return; downloadDirPicker.launched=false; downloadDirPicker.running = true }
  function useDownloads() { if (engine) engine.useDownloads() }
  function sendClipboard() { if (!selectedDevice || clipboard.running || clipboardImage.running) return; clipboardImage.launched=false; clipboardImage.running = true }
  function copyReceivedText() { clipboardWriter.launched=false; clipboardWriter.running=true }
  function copyReceivedFile() { if (receivedFile === "" || fileCopier.running) return; copyNote=strings.t("clipboard.copying"); fileCopier.path=receivedFile; fileCopier.launched=false; fileCopier.running=true }
  onReceivedFileChanged: copyNote=""

  // The kit's Button already paints its own mouse hover from containsMouse.
  // `hasCursor` is this panel's keyboard cursor, and the handlers only ever
  // took it: pointing the cursor at whatever the mouse touched and never
  // letting go, so the last button the pointer crossed stayed lit after it had
  // moved away, and two things looked hovered at once.
  //
  // Taking it on enter and releasing it on leave keeps one highlight, under the
  // pointer. selectedIndex survives the release so an arrow key resumes from
  // where the mouse left off rather than from the top.
  //
  // The release is guarded by index because entering the next button can be
  // delivered before leaving the previous one; unguarded, that order would
  // clear the highlight on the button the pointer is now sitting on.
  function noteHover(on, index) {
    if (on) { cursorActive = true; selectedIndex = index }
    else if (selectedIndex === index) cursorActive = false
  }

  function moveCursor(dx, dy) {
    if (strings.rightToLeft) dx = -dx
    cursorActive = true
    if (viewState === "nearby") {
      // The update row is the last stop until a failure reveals the two
      // recovery actions that follow it.
      var lastNearbyIndex=helperCopyLinkIndex>=0
        ? helperCopyLinkIndex
        : (helperUpdateIndex>=0
          ? helperUpdateIndex
          : (useDownloadsIndex>=0
            ? useDownloadsIndex
            : (receiverEnabled&&backendReady ? devices.length+2 : Math.max(-1,devices.length-1))))
      if (dx !== 0 && helperCopyCommandIndex >= 0 && selectedIndex >= helperCopyCommandIndex) {
        selectedIndex = Math.max(helperCopyCommandIndex, Math.min(helperCopyLinkIndex, selectedIndex + dx))
        return
      }
      if (dx !== 0 && selectedIndex >= devices.length) {
        selectedIndex = Math.max(devices.length, Math.min(lastNearbyIndex, selectedIndex + dx))
        return
      }
      if (dy !== 0) selectedIndex = Math.max(-1, Math.min(lastNearbyIndex, selectedIndex + dy))
      return
    }
    // A received file adds Copy beside Done; either arrow axis moves between
    // the two, since they are the only stops.
    if (viewState === "success" && receivedFile !== "") {
      selectedIndex = Math.max(0, Math.min(1, selectedIndex + (dx !== 0 ? dx : dy)))
      return
    }

    if (dx !== 0 && (viewState === "incoming" || viewState === "text" || viewState === "incoming_pin_disable")) {
      selectedIndex = Math.max(0, Math.min(1, selectedIndex + dx))
      return
    }
    if (dx !== 0 && viewState === "incoming_pin_settings" && incomingPinEnabled && selectedIndex < 2) {
      selectedIndex = Math.max(0, Math.min(1, selectedIndex + dx))
      return
    }
    var count = viewState === "target" ? 4 : ((viewState === "incoming" || viewState === "text" || viewState === "incoming_pin_disable") ? 2 : (viewState === "incoming_pin_settings" ? (incomingPinEnabled ? 3 : 2) : 1))
    if (count > 0 && dy !== 0) selectedIndex = Math.max(0, Math.min(count - 1, selectedIndex + dy))
  }
  function activateCursor() {
    if (viewState === "nearby") {
      // Checked before the action row: with the helper unusable that row is
      // hidden, and the update button takes the index rescan would have had.
      if (selectedIndex < 0) toggleReceiver()
      else if (helperUpdateIndex >= 0 && selectedIndex === helperUpdateIndex) retryHelper()
      else if (helperCopyCommandIndex >= 0 && selectedIndex === helperCopyCommandIndex) copyUpdateCommand()
      else if (helperCopyLinkIndex >= 0 && selectedIndex === helperCopyLinkIndex) copyRepositoryLink()
      else if (useDownloadsIndex >= 0 && selectedIndex === useDownloadsIndex) useDownloads()
      else if (selectedIndex < devices.length) chooseDevice(selectedIndex)
      else if (receiverEnabled && backendReady && selectedIndex === devices.length) forceFullDiscovery()
      else if (receiverEnabled && backendReady && selectedIndex === devices.length+1) openIncomingPinSettings()
      else if (receiverEnabled && backendReady && selectedIndex === devices.length+2) chooseDownloadDir()
    }
    else if (viewState === "target") selectedIndex === 0 ? selectFiles() : (selectedIndex === 1 ? selectFolder() : (selectedIndex === 2 ? sendClipboard() : goBack()))
    else if (viewState === "incoming") selectedIndex === 0 ? declineIncoming() : acceptIncoming()
    else if (viewState === "text") { if(selectedIndex===0)copyReceivedText(); else finishText() }
    else if (viewState === "incoming_pin_settings") incomingPinEnabled ? (selectedIndex===0?beginIncomingPinEdit():(selectedIndex===1?requestDisableIncomingPin():goBack())) : (selectedIndex===0?beginIncomingPinEdit():goBack())
    else if (viewState === "incoming_pin_disable") selectedIndex===0 ? cancelIncomingPinSettings() : confirmDisableIncomingPin()
    else if (viewState === "sending") cancelOutgoing()
    else if (viewState === "success" && receivedFile !== "" && selectedIndex === 0) copyReceivedFile()
    else if (viewState === "success" || viewState === "error") finishTerminal()
  }

  onOpenedChanged: {
    if (opened) {
      cursorActive=false; selectedIndex=-1; nearbyPhraseIndex=0
      syncOpenState()
      Qt.callLater(function(){ if (root.viewState==="pin") pinInput.forceActiveFocus(); else if(root.viewState==="incoming_pin_edit")incomingPinInput.forceActiveFocus(); else keyCatcher.forceActiveFocus() })
    } else { clearSecretInputs(); syncOpenState() }
  }

  Timer {
    id: nearbyPhraseTimer
    interval: 2800
    running: root.rotatingPhrases
    repeat: true
    onTriggered: nearbyPhraseSwap.restart()
  }

  SequentialAnimation {
    id: nearbyPhraseSwap
    PropertyAnimation {
      target: hero; property: "metaOpacity"
      to: 0.0; duration: 180; easing.type: Easing.OutQuad
    }
    ScriptAction {
      script: root.nearbyPhraseIndex = (root.nearbyPhraseIndex + 1) % root.nearbyPhrases.length
    }
    PropertyAnimation {
      target: hero; property: "metaOpacity"
      to: 1.0; duration: 260; easing.type: Easing.InQuad
    }
  }

  onRotatingPhrasesChanged: {
    if (!rotatingPhrases) {
      nearbyPhraseSwap.stop()
      hero.metaOpacity = 1.0
    }
  }

  // Quickshell reports a command it could not launch by returning `running` to
  // false without ever emitting `started` or `exited`, so a missing binary has
  // to be caught there. Exit codes never carry that news: nothing runs a shell
  // on our behalf, so the 127 a shell would report cannot reach us.
  Process {
    id: picker
    property bool launched: false
    command: ["omarchy-file-select","--title",strings.t("chooser.sendFiles"),"--multiple"]
    running: false
    stdout: StdioCollector { id: pickerOutput; waitForEnd: true }
    onStarted: picker.launched=true
    onRunningChanged: if (!running && !picker.launched) root.failWith(strings.t("chooser.notStarted"))
    onExited: function(code) {
      // The chooser separates a decision from a fault: 1 is nobody picking
      // anything, and anything above it is a chooser that never opened.
      if (code > 1) { root.failWith(strings.t("chooser.notOpened")); return }
      if (code !== 0 || !root.selectedDevice) return
      var paths=String(pickerOutput.text || "").split("\n").filter(function(v){return v.trim()!==""})
      if (paths.length) root.beginOutgoing({kind:"files",device:root.selectedDevice,paths:paths})
    }
  }
  Process {
    id: folderPicker
    property bool launched: false
    command: ["omarchy-file-select","--title",strings.t("chooser.sendFolder"),"--directory"]
    running: false
    stdout: StdioCollector { id: folderPickerOutput; waitForEnd: true }
    onStarted: folderPicker.launched=true
    onRunningChanged: if (!running && !folderPicker.launched) root.failWith(strings.t("chooser.notStarted"))
    onExited: function(code) {
      if (code > 1) { root.failWith(strings.t("chooser.notOpened")); return }
      if (code !== 0 || !root.selectedDevice) return
      var paths=String(folderPickerOutput.text || "").split("\n").filter(function(v){return v.trim()!==""})
      if (paths.length) root.beginOutgoing({kind:"files",device:root.selectedDevice,paths:paths})
    }
  }
  // A cancelled chooser changes nothing. A chosen folder goes to the engine,
  // which persists it and restarts the helper to apply it.
  Process {
    id: downloadDirPicker
    property bool launched: false
    command: ["omarchy-file-select","--title",strings.t("chooser.saveTo"),"--directory"]
    running: false
    stdout: StdioCollector { id: downloadDirPickerOutput; waitForEnd: true }
    onStarted: downloadDirPicker.launched=true
    onRunningChanged: if (!running && !downloadDirPicker.launched && root.viewState === "nearby") root.failWith(strings.t("chooser.notStarted"))
    onExited: function(code) {
      if (root.viewState !== "nearby") return
      if (code > 1) { root.failWith(strings.t("chooser.notOpened")); return }
      var path=String(downloadDirPickerOutput.text || "").split("\n")[0]
      if (code === 0 && path !== "" && root.engine) root.engine.setDownloadDir(path)
    }
  }
  // A picture on the clipboard, such as a screenshot, is saved as a file and
  // sent like one. The script exits 3 when the clipboard holds text or no
  // picture, and the text is then read as before: text wins, so copying
  // spreadsheet cells, which also offers a rendering of them, still sends text.
  Process {
    id: clipboardImage
    property bool launched: false
    command: [(root.engine ? root.engine.pluginDir : "") + "/bin/nearby-clipboard-image"]
    running: false
    stdout: StdioCollector { id: clipboardImageOutput; waitForEnd: true }
    onStarted: clipboardImage.launched=true
    onRunningChanged: if (!running && !clipboardImage.launched) root.failWith(strings.t("clipboard.imageFailed"))
    onExited: function(code) {
      var path=String(clipboardImageOutput.text || "").split("\n")[0]
      if (code===3) { clipboard.launched=false; clipboard.running=true; return }
      if (code===0 && path!=="" && root.selectedDevice) root.beginOutgoing({kind:"files",device:root.selectedDevice,paths:[path]})
      else root.failWith(strings.t("clipboard.imageFailed"))
    }
  }
  Process {
    id: clipboard
    property bool launched: false
    command: ["wl-paste","--no-newline","--type","text"]
    running: false
    stdout: StdioCollector { id: clipboardOutput; waitForEnd: true }
    onStarted: clipboard.launched=true
    onRunningChanged: if (!running && !clipboard.launched) root.failWith(strings.t("clipboard.pasteMissing"))
    onExited: function(code) {
      var text=String(clipboardOutput.text || "")
      if (code===0 && text.trim()!=="" && root.selectedDevice) root.beginOutgoing({kind:"text",device:root.selectedDevice,text:text})
      else root.failWith(strings.t("clipboard.empty"))
    }
  }

  BarIconButton {
    id: button; anchors.fill: parent; bar: root.bar
    foreground: root.barIconColor
    iconComponent: Component {
      Item { NearbyIcon { anchors.centerIn: parent; iconSize: Style.bar.iconFont; color: root.barIconColor } }
    }
    active: root.incoming !== null || root.viewState === "receiving" || root.viewState === "sending" || root.viewState === "pin"
    tooltipText: !root.receiverEnabled ? strings.t("bar.turnedOff") : (root.backendReady ? (root.viewState === "pin" ? strings.t("bar.pinRequired") : (root.incoming ? strings.t("status.incomingTransfer") : strings.t("bar.ready"))) : strings.t("bar.unavailable"))
    onPressed: function(code) { root.toggle() }
  }

  KeyboardPanel {
    id: panel; anchorItem: button; owner: root; bar: root.bar; open: root.opened; focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(360))
    contentHeight: panel.fittedContentHeight(content.implicitHeight, Style.space(560))
    PanelKeyCatcher {
      id: keyCatcher; anchors.fill: parent
      blocked: (root.viewState==="pin" && pinInput.activeFocus)||(root.viewState==="incoming_pin_edit"&&incomingPinInput.activeFocus)
      onMoveRequested: function(dx,dy){root.moveCursor(dx,dy)}
      // Keys act only on a visible cursor, as in Omarchy's own panels: the
      // panel takes the keyboard when it opens, and Enter or Space meant for
      // another window must not turn Nearby off or accept a transfer.
      onActivateRequested: if (root.cursorActive) root.activateCursor()
      onCloseRequested: root.goBack()
      Column {
        id: content; width: parent.width; spacing: Style.space(12)
        PanelHero {
          id: hero
          title: root.viewState.indexOf("incoming_pin_")===0 ? strings.t("hero.incomingPin") : ((root.viewState === "target" || root.viewState === "pin") && root.selectedDevice ? root.selectedDevice.alias : (root.viewState === "incoming" ? strings.t("hero.incoming") : "Nearby"))
          meta: root.heroMetaText
          detail: ""
          foreground: root.foreground; fontFamily: root.fontFamily
          iconComponent: root.viewState === "incoming" ? incomingHeroIcon : nearbyHeroIcon
          trailingControl: root.viewState === "nearby" ? receiverToggle : null
        }
        Component { id: nearbyHeroIcon; NearbyIcon { iconSize: Style.font.display; color: root.incoming ? root.urgent : root.foreground } }
        Component { id: incomingHeroIcon; Text { textFormat:Text.PlainText; text: "󰁅"; color: root.incoming ? root.urgent : root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.display } }
        Component {
          id: receiverToggle
          ToggleSwitch {
            checked: root.receiverEnabled
            hasCursor: root.cursorActive && root.selectedIndex < 0
            foreground: root.foreground
            onHovered: function(on) { root.noteHover(on, -1) }
            onToggled: root.toggleReceiver()
            PanelToolTip { visible: parent.containsMouse; text: root.receiverEnabled ? strings.t("receiver.turnOff") : strings.t("receiver.turnOn"); fontFamily:root.fontFamily }
          }
        }
        PanelSeparator { width: parent.width }

        Column {
          visible: root.viewState === "nearby"; width: parent.width; spacing: Style.space(6)
          PanelSectionHeader { text: root.helperBlocksNearby ? strings.t("section.helper") : strings.t("section.devices"); foreground:root.foreground; fontFamily:root.fontFamily }
          // Silent while the helper row owns the view: its text there was
          // errorText, which the row states once already.
          Text { visible: root.devices.length===0 && !root.helperBlocksNearby; width:parent.width; textFormat:Text.PlainText; text: !root.receiverEnabled ? strings.t("devices.off") : (root.discoveryActive ? strings.t("devices.finding") : (root.backendReady ? strings.t("devices.none") : root.errorText)); wrapMode:Text.Wrap; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; topPadding:Style.space(12); bottomPadding:Style.space(12) }
          Repeater {
            model: root.devices
            Button { required property var modelData; required property int index; width:parent.width; leftAlign:true; bordered:false; iconText:Model.iconFor(modelData.deviceType); text:modelData.alias; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===index; onHovered:function(v){root.noteHover(v,index)}; onClicked:root.chooseDevice(index) }
          }
          // The devices are what the panel is for; what follows tunes the
          // receiver. A rule and the dimmed colour Back already uses keep the
          // two from reading as one list.
          PanelSeparator { visible:nearbyActions.visible; width:parent.width }
          Row {
            id:nearbyActions; visible:root.receiverEnabled&&root.backendReady; width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; leftAlign:true; bordered:false; iconText:"󰑐"; text:strings.t("action.rescan"); tooltipText:strings.t("action.rescanTooltip"); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length; onHovered:function(v){root.noteHover(v,root.devices.length)}; onClicked:root.forceFullDiscovery() }
            Button { width:(parent.width-parent.spacing)/2; leftAlign:true; bordered:false; iconText:"󰌾"; text:(root.incomingPinEnabled?strings.t("action.pinOn"):strings.t("action.pinOff")); tooltipText:strings.t("action.pinTooltip"); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length+1; onHovered:function(v){root.noteHover(v,root.devices.length+1)}; onClicked:root.openIncomingPinSettings() }
          }
          Button { visible:nearbyActions.visible; width:parent.width; leftAlign:true; bordered:false; iconText:"󰉋"; text:strings.t("folder.saveTo", [Model.folderName(root.saveDirectory)]); tooltipText:strings.t("folder.saveToTooltip", [root.saveDirectory]); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length+2; onHovered:function(v){root.noteHover(v,root.devices.length+2)}; onClicked:root.chooseDownloadDir() }
          Button { visible:root.useDownloadsIndex>=0; width:parent.width; leftAlign:true; bordered:false; iconText:"󰕌"; text:strings.t("folder.useDownloads"); tooltipText:strings.t("folder.useDownloadsTooltip", [root.downloadDir]); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.useDownloadsIndex; onHovered:function(v){root.noteHover(v,root.useDownloadsIndex)}; onClicked:root.useDownloads() }

          // The checkout pins immutable helper bytes. The launcher resolves
          // them outside the plugin tree, and this action retries/prefetches
          // that exact helper when automatic startup cannot do so.
          Column {
            id: helperUpdate
            visible: root.helperUpdateOffered; width:parent.width; spacing:Style.space(6)
            // Only when it follows something. With the helper blocking the
            // view this row is the section, and a rule under its own header
            // separates nothing.
            PanelSeparator { visible:!root.helperBlocksNearby; width:parent.width }

            // The versions, once. Replaced by live progress while the repair
            // runs so the button is not the only thing that moves.
            Text {
              width:parent.width; textFormat:Text.PlainText; wrapMode:Text.Wrap
              text: root.helperUpdating && root.helperUpdateStatus!=="" ? root.helperUpdateStatus : root.helperUpdateDetail
              color: root.helperUpdating ? root.dim : root.foreground
              font.family:root.fontFamily; font.pixelSize:Style.font.body
            }
            Text {
              visible:!root.helperUpdating; width:parent.width; textFormat:Text.PlainText; wrapMode:Text.Wrap
              text:strings.t("helper.explain")
              color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body
            }
            Button {
              width:parent.width; bordered:true
              iconText: root.helperUpdateError!=="" && !root.helperUpdating ? "󰑐" : "󰇚"
              // After a failure the old label gives no sign the press landed,
              // and the error above it stays put; naming the retry is the only
              // thing that distinguishes "not pressed yet" from "pressed once".
              text: root.helperUpdating ? strings.t("helper.preparing") : (root.helperUpdateError!=="" ? strings.t("helper.tryAgain") : strings.t("helper.retry"))
              tooltipText:strings.t("helper.retryTooltip")
              enabled:!root.helperUpdating
              foreground:root.foreground; fontFamily:root.fontFamily
              hasCursor:root.cursorActive&&root.selectedIndex===root.helperUpdateIndex
              onHovered:function(v){root.noteHover(v,root.helperUpdateIndex)}
              onClicked:root.retryHelper()
            }

            // The fallback is subordinate to the failure, so it sits under a
            // rule of its own rather than continuing the same flat stack.
            Column {
              visible:root.helperUpdateError!==""; width:parent.width; spacing:Style.space(6)
              Text { width:parent.width; textFormat:Text.PlainText; text:root.helperUpdateError; wrapMode:Text.Wrap; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              PanelSeparator { width:parent.width }
              Text { width:parent.width; textFormat:Text.PlainText; text:strings.t("helper.runInTerminal"); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              Text { width:parent.width; textFormat:Text.PlainText; text:root.updateCommand; elide:Text.ElideMiddle; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              Row {
                width:parent.width; spacing:Style.space(8)
                Button { width:(parent.width-parent.spacing)/2; bordered:true; iconText:"󰆏"; text:strings.t("helper.copyCommand"); tooltipText:root.updateCommand; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.helperCopyCommandIndex; onHovered:function(v){root.noteHover(v,root.helperCopyCommandIndex)}; onClicked:root.copyUpdateCommand() }
                Button { width:(parent.width-parent.spacing)/2; bordered:false; iconText:"󰌷"; text:strings.t("helper.copyLink"); tooltipText:root.repositoryUrl; foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.helperCopyLinkIndex; onHovered:function(v){root.noteHover(v,root.helperCopyLinkIndex)}; onClicked:root.copyRepositoryLink() }
              }
              Text { visible:root.copyNote!==""; width:parent.width; textFormat:Text.PlainText; text:root.copyNote; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
            }
          }
        }

        Column {
          visible: root.viewState === "target"; width:parent.width; spacing:Style.space(6)
          PanelSectionHeader { text:strings.t("section.send"); foreground:root.foreground; fontFamily:root.fontFamily }
          Button { width:parent.width; leftAlign:true; iconText:"󰈔"; text:strings.t("send.files"); foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.selectFiles() }
          Button { width:parent.width; leftAlign:true; iconText:"󰉋"; text:strings.t("send.folder"); foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.selectFolder() }
          Button { width:parent.width; leftAlign:true; iconText:"󰅇"; text:strings.t("send.clipboard"); foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===2; onHovered:function(v){root.noteHover(v,2)}; onClicked:root.sendClipboard() }
          Button { width:parent.width; leftAlign:true; bordered:false; iconText:strings.rightToLeft?"󰅂":"󰅁"; text:strings.t("action.back"); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===3; onHovered:function(v){root.noteHover(v,3)}; onClicked:root.goBack() }
        }

        Column {
          visible: root.viewState === "pin"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:strings.t("section.receiverPin"); foreground:root.foreground; fontFamily:root.fontFamily }
          Text { textFormat:Text.PlainText; width:parent.width; text:strings.t("pin.receiverRequires"); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          TextField {
            id: pinInput; width:parent.width; password:true; placeholderText:strings.t("pin.placeholder"); foreground:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body
            onAccepted: root.retryWithPin()
            Keys.onPressed: function(event) { if(event.key===Qt.Key_Escape){root.cancelPin();event.accepted=true} }
          }
          Text { textFormat:Text.PlainText; visible:root.pinError!==""; width:parent.width; text:root.pinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.cancel"); bordered:true; foreground:root.dim; onClicked:root.cancelPin() }
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.retry"); bordered:true; foreground:root.foreground; onClicked:root.retryWithPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming_pin_settings"; width:parent.width; spacing:Style.space(8)
          Button { visible:!root.incomingPinEnabled; width:parent.width; text:strings.t("pin.enable"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.beginIncomingPinEdit() }
          Row { visible:root.incomingPinEnabled; width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("pin.change"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.beginIncomingPinEdit() }
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("pin.disable"); bordered:true; foreground:root.urgent; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.requestDisableIncomingPin() }
          }
          Button { width:parent.width; leftAlign:true; bordered:false; iconText:strings.rightToLeft?"󰅂":"󰅁"; text:strings.t("action.back"); foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===(root.incomingPinEnabled?2:1); onHovered:function(v){root.noteHover(v,root.incomingPinEnabled?2:1)}; onClicked:root.goBack() }
          Text { textFormat:Text.PlainText; visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
        }

        Column {
          visible: root.viewState === "incoming_pin_edit"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:root.incomingPinEnabled?strings.t("section.changePin"):strings.t("section.enablePin"); foreground:root.foreground; fontFamily:root.fontFamily }
          Text { textFormat:Text.PlainText; width:parent.width; text:strings.t("pin.rules"); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          TextField {
            id:incomingPinInput; width:parent.width; password:true; placeholderText:strings.t("pin.newPlaceholder"); maximumLength:64; enabled:!root.incomingPinUpdating; foreground:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body
            validator:RegularExpressionValidator { regularExpression:/[A-Za-z0-9._~-]{0,64}/ }
            onAccepted:root.submitIncomingPin()
            Keys.onPressed:function(event){if(event.key===Qt.Key_Escape){root.cancelIncomingPinSettings();event.accepted=true}}
          }
          Text { textFormat:Text.PlainText; visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.cancel"); bordered:true; enabled:!root.incomingPinUpdating; foreground:root.dim; onClicked:root.cancelIncomingPinSettings() }
            Button { width:(parent.width-parent.spacing)/2; text:root.incomingPinUpdating?strings.t("action.saving"):strings.t("action.save"); bordered:true; enabled:!root.incomingPinUpdating; foreground:root.foreground; onClicked:root.submitIncomingPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming_pin_disable"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:strings.t("section.disablePin"); foreground:root.foreground; fontFamily:root.fontFamily }
          Text { textFormat:Text.PlainText; width:parent.width; text:strings.t("pin.disableDetail"); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Text { textFormat:Text.PlainText; visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.cancel"); bordered:true; enabled:!root.incomingPinUpdating; foreground:root.dim; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.cancelIncomingPinSettings() }
            Button { width:(parent.width-parent.spacing)/2; text:root.incomingPinUpdating?strings.t("action.disabling"):strings.t("action.disable"); bordered:true; enabled:!root.incomingPinUpdating; foreground:root.urgent; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.confirmDisableIncomingPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming" && root.incoming; width:parent.width; spacing:Style.space(8)
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incoming ? strings.t("incoming.wantsToSend", [root.incoming.sender]) : ""; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { visible:root.incomingMessage===""; width:parent.width; textFormat:Text.PlainText; text:root.incoming ? Model.incomingSummary(root.incoming.files, strings)+" · "+Model.formatBytes(root.incoming.total, strings) : ""; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; elide:Text.ElideRight }
          // A message is shown before it is accepted, so the choice is informed.
          Text { visible:root.incomingMessage!==""; width:parent.width; textFormat:Text.PlainText; text:root.incomingMessage; wrapMode:Text.Wrap; maximumLineCount:4; elide:Text.ElideRight; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Text { textFormat:Text.PlainText; visible:root.incomingQueue.length>1; width:parent.width; text:strings.tn("incoming.moreRequests", root.incomingQueue.length-1, [strings.number(root.incomingQueue.length-1)]); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.decline"); foreground:root.urgent; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.declineIncoming() }
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.accept"); foreground:root.foreground; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.acceptIncoming() }
          }
        }

        Column {
          visible: root.viewState==="sending"||root.viewState==="receiving"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:root.viewState==="sending" ? strings.t("section.sending") : strings.t("section.receiving"); foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.transferName; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true; elide:Text.ElideMiddle }
          Rectangle { width:parent.width; height:Style.space(4); radius:height/2; color:Qt.darker(root.foreground,2.2); Rectangle { width:parent.width*Math.max(0,Math.min(1,root.progress)); height:parent.height; radius:height/2; color:root.foreground; Behavior on width { NumberAnimation { duration:120 } } } }
          Text { textFormat:Text.PlainText; text:strings.t(root.viewState==="sending"?"transfer.progressTo":"transfer.progressFrom", [strings.number(Math.round(root.progress*100)), root.transferPeer]); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Text { visible:root.progressDetail!==""; textFormat:Text.PlainText; text:root.progressDetail; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { visible:root.viewState==="sending"; text:strings.t("action.cancel"); bordered:true; foreground:root.urgent; hasCursor:root.cursorActive; onClicked:root.cancelOutgoing() }
        }

        Column {
          visible: root.viewState==="success"||root.viewState==="error"; width:parent.width; spacing:Style.space(8)
          Text { width:parent.width; textFormat:Text.PlainText; text:root.viewState==="success" ? root.statusText : root.errorText; wrapMode:Text.Wrap; color:root.viewState==="error"?root.urgent:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { visible:root.viewState==="success"; width:parent.width; textFormat:Text.PlainText; text:root.transferPeer!=="" ? strings.t(root.engine && root.engine.transferWasOutgoing ? "transfer.to" : "transfer.from", [root.transferPeer]) : ""; wrapMode:Text.Wrap; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          // One received file can be looked at and copied without leaving the
          // panel. The preview is decoded off the UI thread, at the size it is
          // drawn, and only for the picture types the engine identified. A
          // phone format the shell cannot decode arrives converted to a PNG.
          Image {
            visible:root.receivedPreview!=="" && status===Image.Ready
            width:parent.width; height:Math.min(Style.space(200), width*implicitHeight/Math.max(1,implicitWidth))
            source:root.receivedPreview!=="" ? Model.fileUrl(root.receivedPreview) : ""
            sourceSize.width:2*width; sourceSize.height:2*Style.space(200)
            fillMode:Image.PreserveAspectFit; asynchronous:true; cache:false; autoTransform:true
          }
          Text { visible:root.receivedFile!==""; width:parent.width; textFormat:Text.PlainText; text:root.receivedFile.slice(root.receivedFile.lastIndexOf("/")+1); elide:Text.ElideMiddle; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { visible:root.receivedFile!==""; width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.copy"); iconText:"󰆏"; tooltipText:root.receivedIsImage?strings.t("clipboard.copyPicture"):strings.t("clipboard.copyFile"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.copyReceivedFile() }
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.done"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.finishTerminal() }
          }
          Text { visible:root.receivedFile!==""&&root.copyNote!==""; width:parent.width; textFormat:Text.PlainText; text:root.copyNote; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { visible:root.receivedFile===""; text:strings.t("action.done"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive; onClicked:root.finishTerminal() }
        }

        Column {
          visible: root.viewState==="text"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:strings.t("section.receivedText"); foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incomingText; wrapMode:Text.Wrap; maximumLineCount:6; elide:Text.ElideRight; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.copy"); iconText:"󰆏"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:{root.copyReceivedText()} }
            Button { width:(parent.width-parent.spacing)/2; text:strings.t("action.done"); bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.finishText() }
          }
        }
      }
    }
  }

  // The text goes out as an argument rather than through stdin: none of it is
  // secret, and it keeps this independent of clipboardWriter, which holds
  // mid-transfer state the update row must not disturb.
  Process {
    id: textCopier
    property bool launched: false
    property string payload: ""
    command: ["wl-copy", textCopier.payload]
    running: false
    onStarted: textCopier.launched=true
    onRunningChanged: if (!running && !textCopier.launched) root.copyNote=strings.t("clipboard.copyMissing")
    onExited: function(code) { root.copyNote = code===0 ? strings.t("clipboard.copied") : strings.t("clipboard.copyMissing") }
  }

  // A received file is copied by a script: a picture has to be converted and
  // redirected into wl-copy, which a Process cannot do by itself.
  Process {
    id: fileCopier
    property bool launched: false
    property string path: ""
    command: [(root.engine ? root.engine.pluginDir : "") + "/bin/nearby-copy-file", fileCopier.path]
    running: false
    onStarted: fileCopier.launched=true
    onRunningChanged: if (!running && !fileCopier.launched && root.viewState === "success" && root.receivedFile !== "" && fileCopier.path === root.receivedFile) root.copyNote=strings.t("clipboard.copyNotStarted")
    onExited: function(code) { if (root.viewState !== "success" || root.receivedFile === "" || fileCopier.path !== root.receivedFile) return; root.copyNote = code===0 ? strings.t("clipboard.copied") : strings.t("clipboard.copyFailed") }
  }

  Process {
    id: clipboardWriter
    property bool launched: false
    command: ["wl-copy"]
    running: false
    stdinEnabled: true
    onStarted: { clipboardWriter.launched=true; write(root.incomingText); stdinEnabled=false }
    onRunningChanged: if (!running && !clipboardWriter.launched && root.viewState==="text") root.failWith(strings.t("clipboard.copyTextMissing"))
    onExited: function(code) { stdinEnabled=true; if(root.viewState!=="text")return; if(code===0){root.noteTextCopied()} else root.failWith(strings.t("clipboard.copyTextMissing")) }
  }
}
