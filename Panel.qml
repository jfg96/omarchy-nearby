import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons
import "Model.js" as Model

Panel {
  id: root
  moduleName: "oma.nearby"
  // The `oma.nearby` IPC target belongs to the service, which exists once. A
  // widget registering it would register it once per monitor.
  manageIpc: false

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

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.4)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  // Engine state, read under the names the popup already used. A view with no
  // engine says so rather than rendering an empty panel.
  readonly property bool backendReady: engine ? engine.backendReady : false
  readonly property bool receiverEnabled: engine ? engine.receiverEnabled : false
  readonly property bool discoveryActive: engine ? engine.discoveryActive : false
  readonly property var devices: engine ? engine.devices : []
  readonly property var selectedDevice: engine ? engine.selectedDevice : null
  readonly property string viewState: engine ? engine.viewState : "nearby"
  readonly property string statusText: engine ? engine.statusText : "Nearby engine is not loaded."
  readonly property string errorText: engine ? engine.errorText : "Nearby engine is not loaded."
  readonly property var incomingQueue: engine ? engine.incomingQueue : []
  readonly property var incoming: engine ? engine.incoming : null
  readonly property string incomingText: engine ? engine.incomingText : ""
  readonly property real progress: engine ? engine.progress : 0
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

  // Shown only when the in-panel update cannot do the job, so the user still
  // has somewhere to go: the repository and the command that does it by hand.
  readonly property string repositoryUrl: "https://github.com/jfg96/omarchy-nearby"
  readonly property string installerCommand: "~/.config/omarchy/plugins/oma.nearby/install.sh"

  // The update row is the last thing in the Nearby view, so it sits after the
  // devices and after the compact action row when that row is there at all.
  readonly property int helperUpdateIndex: !helperUpdateOffered
    ? -1
    : (receiverEnabled && backendReady ? devices.length + 2 : devices.length)

  // Cursor and popup focus are per monitor, so they stay here.
  property int selectedIndex: 0
  property bool cursorActive: false
  property int nearbyPhraseIndex: 0
  property bool viewRegistered: false
  property string copyNote: ""

  readonly property var nearbyPhrases: [
    "Looking nearby",
    "Finding devices",
    "Listening locally",
    "Ready to receive",
    "Watching the LAN",
    "Checking the air"
  ]
  readonly property bool rotatingPhrases: opened && receiverEnabled && backendReady && viewState === "nearby" && errorText === ""
  readonly property string heroMetaText: {
    if (viewState === "nearby") {
      if (!receiverEnabled) return "Turned off"
      // statusText, never errorText: this line is uppercased and letterspaced,
      // so a full sentence crops mid-word and the body below is already saying
      // the same thing in full.
      if (!backendReady) return statusText
      return nearbyPhrases[nearbyPhraseIndex % nearbyPhrases.length]
    }
    if (viewState === "incoming_pin_settings") return incomingPinEnabled ? "Protection enabled" : "Protection disabled"
    if (viewState === "incoming_pin_edit") return incomingPinEnabled ? "Change protection" : "Enable protection"
    if (viewState === "incoming_pin_disable") return "Confirm disable"
    if (viewState === "target") return "Ready to receive"
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
  function updateHelper() { copyNote=""; if (engine) engine.startHelperUpdate() }
  function copyText(value) { if (textCopier.running) return; copyNote=""; textCopier.payload=String(value); textCopier.launched=false; textCopier.running=true }
  function copyInstallerCommand() { copyText(installerCommand) }
  function copyRepositoryLink() { copyText(repositoryUrl) }
  function failWith(message) { if (engine) engine.failWith(message) }
  function noteTextCopied() { if (engine) engine.noteTextCopied() }
  function beginOutgoing(pending) { if (engine) engine.beginOutgoing(pending) }
  function goBack() { if (viewState === "pin") cancelPin(); else if (viewState.indexOf("incoming_pin_")===0) cancelIncomingPinSettings(); else if (viewState === "target") { if (engine) engine.clearTarget() } else close() }

  // The chooser and the clipboard belong to the monitor the user acted on, so
  // they stay with the view and hand their result to the engine.
  function selectFiles() { if (!selectedDevice || picker.running) return; picker.launched=false; picker.running = true }
  function sendClipboard() { if (!selectedDevice || clipboard.running) return; clipboard.launched=false; clipboard.running = true }
  function copyReceivedText() { clipboardWriter.launched=false; clipboardWriter.running=true }

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
    cursorActive = true
    if (viewState === "nearby") {
      // The update row, when it is showing, is the last stop in either
      // direction: without it a panel reporting an unusable helper has nothing
      // below the receiver switch to reach.
      var lastNearbyIndex=helperUpdateIndex>=0
        ? helperUpdateIndex
        : (receiverEnabled&&backendReady ? devices.length+1 : Math.max(-1,devices.length-1))
      if (dx !== 0 && selectedIndex >= devices.length) {
        selectedIndex = Math.max(devices.length, Math.min(lastNearbyIndex, selectedIndex + dx))
        return
      }
      if (dy !== 0) selectedIndex = Math.max(-1, Math.min(lastNearbyIndex, selectedIndex + dy))
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
    var count = viewState === "target" ? 3 : ((viewState === "incoming" || viewState === "text" || viewState === "incoming_pin_disable") ? 2 : (viewState === "incoming_pin_settings" ? (incomingPinEnabled ? 3 : 2) : 1))
    if (count > 0 && dy !== 0) selectedIndex = Math.max(0, Math.min(count - 1, selectedIndex + dy))
  }
  function activateCursor() {
    if (viewState === "nearby") {
      // Checked before the action row: with the helper unusable that row is
      // hidden, and the update button takes the index rescan would have had.
      if (selectedIndex < 0) toggleReceiver()
      else if (helperUpdateIndex >= 0 && selectedIndex === helperUpdateIndex) updateHelper()
      else if (selectedIndex < devices.length) chooseDevice(selectedIndex)
      else if (receiverEnabled && backendReady && selectedIndex === devices.length) forceFullDiscovery()
      else if (receiverEnabled && backendReady && selectedIndex === devices.length+1) openIncomingPinSettings()
    }
    else if (viewState === "target") selectedIndex === 0 ? selectFiles() : (selectedIndex === 1 ? sendClipboard() : goBack())
    else if (viewState === "incoming") selectedIndex === 0 ? declineIncoming() : acceptIncoming()
    else if (viewState === "text") { if(selectedIndex===0)copyReceivedText(); else finishText() }
    else if (viewState === "incoming_pin_settings") incomingPinEnabled ? (selectedIndex===0?beginIncomingPinEdit():(selectedIndex===1?requestDisableIncomingPin():goBack())) : (selectedIndex===0?beginIncomingPinEdit():goBack())
    else if (viewState === "incoming_pin_disable") selectedIndex===0 ? cancelIncomingPinSettings() : confirmDisableIncomingPin()
    else if (viewState === "sending") cancelOutgoing()
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
    command: ["omarchy-file-select","--title","Send nearby","--multiple"]
    running: false
    stdout: StdioCollector { id: pickerOutput; waitForEnd: true }
    onStarted: picker.launched=true
    onRunningChanged: if (!running && !picker.launched) root.failWith("The file chooser could not be started")
    onExited: function(code) {
      // The chooser separates a decision from a fault: 1 is nobody picking
      // anything, and anything above it is a chooser that never opened.
      if (code > 1) { root.failWith("The file chooser did not open"); return }
      if (code !== 0 || !root.selectedDevice) return
      var paths=String(pickerOutput.text || "").split("\n").filter(function(v){return v.trim()!==""})
      if (paths.length) root.beginOutgoing({kind:"files",device:root.selectedDevice,paths:paths})
    }
  }
  Process {
    id: clipboard
    property bool launched: false
    command: ["wl-paste","--no-newline","--type","text"]
    running: false
    stdout: StdioCollector { id: clipboardOutput; waitForEnd: true }
    onStarted: clipboard.launched=true
    onRunningChanged: if (!running && !clipboard.launched) root.failWith("wl-paste is required to read the clipboard")
    onExited: function(code) {
      var text=String(clipboardOutput.text || "")
      if (code===0 && text.trim()!=="" && root.selectedDevice) root.beginOutgoing({kind:"text",device:root.selectedDevice,text:text})
      else root.failWith("Clipboard is empty")
    }
  }

  BarIconButton {
    id: button; anchors.fill: parent; bar: root.bar; text: "󰀂"
    active: root.incoming !== null || root.viewState === "receiving" || root.viewState === "sending" || root.viewState === "pin"
    tooltipText: !root.receiverEnabled ? "Nearby · Turned off" : (root.backendReady ? (root.viewState === "pin" ? "Nearby · PIN required" : (root.incoming ? "Incoming transfer" : "Nearby · Ready to receive")) : "Nearby · Receiver unavailable")
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
      onActivateRequested: root.activateCursor()
      onCloseRequested: root.goBack()
      Column {
        id: content; width: parent.width; spacing: Style.space(12)
        PanelHero {
          id: hero
          title: root.viewState.indexOf("incoming_pin_")===0 ? "Incoming PIN" : ((root.viewState === "target" || root.viewState === "pin") && root.selectedDevice ? root.selectedDevice.alias : (root.viewState === "incoming" ? "Incoming" : "Nearby"))
          meta: root.heroMetaText
          detail: ""
          foreground: root.foreground; fontFamily: root.fontFamily
          iconComponent: Component { Text { text: root.viewState === "incoming" ? "󰁅" : "󰀂"; color: root.incoming ? root.urgent : root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.display } }
          trailingControl: root.viewState === "nearby" ? receiverToggle : null
        }
        Component {
          id: receiverToggle
          ToggleSwitch {
            checked: root.receiverEnabled
            hasCursor: root.cursorActive && root.selectedIndex < 0
            foreground: root.foreground
            onHovered: function(on) { root.noteHover(on, -1) }
            onToggled: root.toggleReceiver()
            PanelToolTip { visible: parent.containsMouse; text: root.receiverEnabled ? "Turn Nearby off" : "Turn Nearby on"; fontFamily:root.fontFamily }
          }
        }
        PanelSeparator { width: parent.width }

        Column {
          visible: root.viewState === "nearby"; width: parent.width; spacing: Style.space(6)
          PanelSectionHeader { text: root.helperBlocksNearby ? "HELPER" : "DEVICES"; foreground:root.foreground; fontFamily:root.fontFamily }
          // Silent while the helper row owns the view: its text there was
          // errorText, which the row states once already.
          Text { visible: root.devices.length===0 && !root.helperBlocksNearby; width:parent.width; textFormat:Text.PlainText; text: !root.receiverEnabled ? "Nearby is turned off" : (root.discoveryActive ? "Finding devices…" : (root.backendReady ? "No devices nearby" : root.errorText)); wrapMode:Text.Wrap; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; topPadding:Style.space(12); bottomPadding:Style.space(12) }
          Repeater {
            model: root.devices
            Button { required property var modelData; required property int index; width:parent.width; leftAlign:true; bordered:false; iconText:Model.iconFor(modelData.deviceType); text:modelData.alias; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===index; onHovered:function(v){root.noteHover(v,index)}; onClicked:root.chooseDevice(index) }
          }
          Row {
            id:nearbyActions; visible:root.receiverEnabled&&root.backendReady; width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; bordered:false; iconText:"󰑐"; text:"Rescan"; tooltipText:"Search for new devices"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length; onHovered:function(v){root.noteHover(v,root.devices.length)}; onClicked:root.forceFullDiscovery() }
            Button { width:(parent.width-parent.spacing)/2; bordered:false; iconText:"󰌾"; text:"PIN · "+(root.incomingPinEnabled?"On":"Off"); tooltipText:"Incoming PIN settings"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length+1; onHovered:function(v){root.noteHover(v,root.devices.length+1)}; onClicked:root.openIncomingPinSettings() }
          }

          // `omarchy plugin update` fast-forwards the checkout and stops there.
          // The helper is a release asset and bin/ is not tracked, so a source
          // update always leaves the previous binary in place and there is no
          // hook that could fetch the new one. That gap is closable from here:
          // the button replaces only the binary, which is the half Omarchy
          // does not move. The repository and the manual command appear when
          // it cannot -- no arch build published, no network, a checkout on a
          // development version that has no release at all.
          Column {
            id: helperUpdate
            visible: root.helperUpdateOffered; width:parent.width; spacing:Style.space(6)
            // Only when it follows something. With the helper blocking the
            // view this row is the section, and a rule under its own header
            // separates nothing.
            PanelSeparator { visible:!root.helperBlocksNearby; width:parent.width }

            // The versions, once. Replaced by live progress while the updater
            // runs so the button is not the only thing that moves.
            Text {
              width:parent.width; textFormat:Text.PlainText; wrapMode:Text.Wrap
              text: root.helperUpdating && root.helperUpdateStatus!=="" ? root.helperUpdateStatus : root.helperUpdateDetail
              color: root.helperUpdating ? root.dim : root.foreground
              font.family:root.fontFamily; font.pixelSize:Style.font.body
            }
            Text {
              visible:!root.helperUpdating; width:parent.width; textFormat:Text.PlainText; wrapMode:Text.Wrap
              text:"Updating the plugin cannot replace the helper binary."
              color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body
            }
            Button {
              width:parent.width; bordered:true
              iconText: root.helperUpdateError!=="" && !root.helperUpdating ? "󰑐" : "󰇚"
              // After a failure the old label gives no sign the press landed,
              // and the error above it stays put; naming the retry is the only
              // thing that distinguishes "not pressed yet" from "pressed once".
              text: root.helperUpdating ? "Updating…" : (root.helperUpdateError!=="" ? "Try again" : "Update helper")
              tooltipText:"Download the helper that matches this version"
              enabled:!root.helperUpdating
              foreground:root.foreground; fontFamily:root.fontFamily
              hasCursor:root.cursorActive&&root.selectedIndex===root.helperUpdateIndex
              onHovered:function(v){root.noteHover(v,root.helperUpdateIndex)}
              onClicked:root.updateHelper()
            }

            // The fallback is subordinate to the failure, so it sits under a
            // rule of its own rather than continuing the same flat stack.
            Column {
              visible:root.helperUpdateError!==""; width:parent.width; spacing:Style.space(6)
              Text { width:parent.width; textFormat:Text.PlainText; text:root.helperUpdateError; wrapMode:Text.Wrap; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              PanelSeparator { width:parent.width }
              Text { width:parent.width; textFormat:Text.PlainText; text:"Or run this in a terminal:"; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              // Elided rather than wrapped: wrapping broke the path across two
              // lines and orphaned the `.sh`, which reads like a typo and is
              // one. The full text goes to the clipboard, not to the eye.
              Text { width:parent.width; textFormat:Text.PlainText; text:root.installerCommand; elide:Text.ElideMiddle; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
              Row {
                width:parent.width; spacing:Style.space(8)
                Button { width:(parent.width-parent.spacing)/2; bordered:true; iconText:"󰆏"; text:"Copy command"; tooltipText:root.installerCommand; foreground:root.foreground; fontFamily:root.fontFamily; onClicked:root.copyInstallerCommand() }
                Button { width:(parent.width-parent.spacing)/2; bordered:false; iconText:"󰌷"; text:"Copy link"; tooltipText:root.repositoryUrl; foreground:root.dim; fontFamily:root.fontFamily; onClicked:root.copyRepositoryLink() }
              }
              Text { visible:root.copyNote!==""; width:parent.width; textFormat:Text.PlainText; text:root.copyNote; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
            }
          }
        }

        Column {
          visible: root.viewState === "target"; width:parent.width; spacing:Style.space(6)
          PanelSectionHeader { text:"SEND"; foreground:root.foreground; fontFamily:root.fontFamily }
          Button { width:parent.width; leftAlign:true; iconText:"󰈔"; text:"Send files"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.selectFiles() }
          Button { width:parent.width; leftAlign:true; iconText:"󰅇"; text:"Send clipboard"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.sendClipboard() }
          Button { width:parent.width; leftAlign:true; bordered:false; iconText:"󰅁"; text:"Back"; foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===2; onHovered:function(v){root.noteHover(v,2)}; onClicked:root.goBack() }
        }

        Column {
          visible: root.viewState === "pin"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:"RECEIVER PIN"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; text:"This receiver requires a PIN"; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          TextField {
            id: pinInput; width:parent.width; password:true; placeholderText:"PIN"; foreground:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body
            onAccepted: root.retryWithPin()
            Keys.onPressed: function(event) { if(event.key===Qt.Key_Escape){root.cancelPin();event.accepted=true} }
          }
          Text { visible:root.pinError!==""; width:parent.width; text:root.pinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Cancel"; bordered:true; foreground:root.dim; onClicked:root.cancelPin() }
            Button { width:(parent.width-parent.spacing)/2; text:"Retry"; bordered:true; foreground:root.foreground; onClicked:root.retryWithPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming_pin_settings"; width:parent.width; spacing:Style.space(8)
          Button { visible:!root.incomingPinEnabled; width:parent.width; text:"Enable PIN"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.beginIncomingPinEdit() }
          Row { visible:root.incomingPinEnabled; width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Change PIN"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.beginIncomingPinEdit() }
            Button { width:(parent.width-parent.spacing)/2; text:"Disable PIN"; bordered:true; foreground:root.urgent; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.requestDisableIncomingPin() }
          }
          Button { width:parent.width; leftAlign:true; bordered:false; iconText:"󰅁"; text:"Back"; foreground:root.dim; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===(root.incomingPinEnabled?2:1); onHovered:function(v){root.noteHover(v,root.incomingPinEnabled?2:1)}; onClicked:root.goBack() }
          Text { visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
        }

        Column {
          visible: root.viewState === "incoming_pin_edit"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:root.incomingPinEnabled?"CHANGE PIN":"ENABLE PIN"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; text:"Use 1–64 letters, numbers, dot, underscore, tilde or hyphen"; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          TextField {
            id:incomingPinInput; width:parent.width; password:true; placeholderText:"New PIN"; maximumLength:64; enabled:!root.incomingPinUpdating; foreground:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body
            validator:RegularExpressionValidator { regularExpression:/[A-Za-z0-9._~-]{0,64}/ }
            onAccepted:root.submitIncomingPin()
            Keys.onPressed:function(event){if(event.key===Qt.Key_Escape){root.cancelIncomingPinSettings();event.accepted=true}}
          }
          Text { visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Cancel"; bordered:true; enabled:!root.incomingPinUpdating; foreground:root.dim; onClicked:root.cancelIncomingPinSettings() }
            Button { width:(parent.width-parent.spacing)/2; text:root.incomingPinUpdating?"Saving…":"Save"; bordered:true; enabled:!root.incomingPinUpdating; foreground:root.foreground; onClicked:root.submitIncomingPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming_pin_disable"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:"DISABLE PIN"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; text:"New incoming requests will no longer require a PIN."; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Text { visible:root.incomingPinError!==""; width:parent.width; text:root.incomingPinError; color:root.urgent; font.family:root.fontFamily; font.pixelSize:Style.font.body; wrapMode:Text.Wrap }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Cancel"; bordered:true; enabled:!root.incomingPinUpdating; foreground:root.dim; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.cancelIncomingPinSettings() }
            Button { width:(parent.width-parent.spacing)/2; text:root.incomingPinUpdating?"Disabling…":"Disable"; bordered:true; enabled:!root.incomingPinUpdating; foreground:root.urgent; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.confirmDisableIncomingPin() }
          }
        }

        Column {
          visible: root.viewState === "incoming" && root.incoming; width:parent.width; spacing:Style.space(8)
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incoming ? root.incoming.sender+" wants to send" : ""; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incoming ? Model.incomingSummary(root.incoming.files)+" · "+Model.formatBytes(root.incoming.total) : ""; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; elide:Text.ElideRight }
          Text { visible:root.incomingQueue.length>1; width:parent.width; text:(root.incomingQueue.length-1)+(root.incomingQueue.length===2 ? " more request" : " more requests"); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Decline"; foreground:root.urgent; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:root.declineIncoming() }
            Button { width:(parent.width-parent.spacing)/2; text:"Accept"; foreground:root.foreground; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.acceptIncoming() }
          }
        }

        Column {
          visible: root.viewState==="sending"||root.viewState==="receiving"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:root.viewState==="sending" ? "SENDING" : "RECEIVING"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.transferName; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true; elide:Text.ElideMiddle }
          Rectangle { width:parent.width; height:Style.space(4); radius:height/2; color:Qt.darker(root.foreground,2.2); Rectangle { width:parent.width*Math.max(0,Math.min(1,root.progress)); height:parent.height; radius:height/2; color:root.foreground; Behavior on width { NumberAnimation { duration:120 } } } }
          Text { text:Math.round(root.progress*100)+"% · "+(root.viewState==="sending"?"to ":"from ")+root.transferPeer; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { visible:root.viewState==="sending"; text:"Cancel"; bordered:true; foreground:root.urgent; hasCursor:root.cursorActive; onClicked:root.cancelOutgoing() }
        }

        Column {
          visible: root.viewState==="success"||root.viewState==="error"; width:parent.width; spacing:Style.space(8)
          Text { width:parent.width; textFormat:Text.PlainText; text:root.viewState==="success" ? root.statusText : root.errorText; wrapMode:Text.Wrap; color:root.viewState==="error"?root.urgent:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { visible:root.viewState==="success"; width:parent.width; textFormat:Text.PlainText; text:root.transferPeer!=="" ? (root.statusText==="Sent"?"to ":"from ")+root.transferPeer : ""; wrapMode:Text.Wrap; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { text:"Done"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive; onClicked:root.finishTerminal() }
        }

        Column {
          visible: root.viewState==="text"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:"RECEIVED TEXT"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incomingText; wrapMode:Text.Wrap; maximumLineCount:6; elide:Text.ElideRight; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Copy"; iconText:"󰆏"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){root.noteHover(v,0)}; onClicked:{root.copyReceivedText()} }
            Button { width:(parent.width-parent.spacing)/2; text:"Done"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){root.noteHover(v,1)}; onClicked:root.finishText() }
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
    onRunningChanged: if (!running && !textCopier.launched) root.copyNote="wl-copy is required to copy"
    onExited: function(code) { root.copyNote = code===0 ? "Copied to clipboard" : "wl-copy is required to copy" }
  }

  Process {
    id: clipboardWriter
    property bool launched: false
    command: ["wl-copy"]
    running: false
    stdinEnabled: true
    onStarted: { clipboardWriter.launched=true; write(root.incomingText); stdinEnabled=false }
    onRunningChanged: if (!running && !clipboardWriter.launched && root.viewState==="text") root.failWith("wl-copy is required to copy received text")
    onExited: function(code) { stdinEnabled=true; if(root.viewState!=="text")return; if(code===0){root.noteTextCopied()} else root.failWith("wl-copy is required to copy received text") }
  }
}
