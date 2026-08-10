import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons
import "Model.js" as Model

Panel {
  id: root
  moduleName: "oma.nearby"
  ipcTarget: "oma.nearby"
  manageIpc: false

  // The host may replace moduleName with an instance id. Keep the manifest id
  // stable for registry lookups and filesystem paths.
  readonly property string manifestPluginId: "oma.nearby"
  readonly property var manifestMetadata: bar && bar.shell
    ? bar.shell.barWidgetRegistry.metadataFor(manifestPluginId)
    : null
  readonly property string metadataSourceDir: manifestMetadata
    ? String(manifestMetadata.sourceDir || "")
    : ""
  readonly property string pluginDir: metadataSourceDir !== ""
    ? metadataSourceDir
    : (Quickshell.env("HOME") || "") + "/.config/omarchy/plugins/" + manifestPluginId
  property string pluginVersion: ""
  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.4)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  property bool backendReady: false
  property bool receiverEnabled: setting("receiverEnabled", true) !== false
  property bool discoveryActive: false
  property var devices: []
  property var selectedDevice: null
  property string viewState: "nearby"
  property string statusText: "Starting receiver…"
  property string errorText: ""
  property int selectedIndex: 0
  property bool cursorActive: false
  property var incoming: null
  property string incomingText: ""
  property string lastReceivedPath: ""
  property real progress: 0
  property string transferName: ""
  property string transferPeer: ""
  property string activeIncomingSession: ""
  property string outgoingTransferId: ""
  property int transferSequence: 0
  property int nearbyPhraseIndex: 0
  property bool shutdownPending: false
  property bool backendAcceptedThisRun: false
  property bool backendVersionMismatch: false

  FileView {
    path: root.pluginDir + "/manifest.json"
    printErrors: false
    onLoaded: {
      root.pluginVersion = Model.manifestVersion(text(), root.manifestPluginId)
      if (root.pluginVersion === "") {
        root.errorText = "Nearby manifest version unavailable."
        root.statusText = root.errorText
      }
    }
    onLoadFailed: function(error) {
      root.pluginVersion = ""
      root.errorText = "Nearby manifest version unavailable."
      root.statusText = root.errorText
    }
  }

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
      if (!backendReady) return errorText || statusText
      return nearbyPhrases[nearbyPhraseIndex % nearbyPhrases.length]
    }
    if (viewState === "target") return "Ready to receive"
    return statusText
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  IpcHandler {
    target: root.ipcTarget
    function open(): string { root.open(); return "ok" }
    function close(): string { root.close(); return "ok" }
    function toggle(): string { root.toggle(); return "ok" }
    function receiverOn(): string { if (!root.receiverEnabled) root.toggleReceiver(); return "ok" }
    function receiverOff(): string { if (root.receiverEnabled) root.toggleReceiver(); return "ok" }
    function receiverToggle(): string { root.toggleReceiver(); return "ok" }
    function status(): string { return JSON.stringify({enabled:root.receiverEnabled,ready:root.backendReady,running:backend.running,devices:root.devices.length}) }
  }

  function send(command) {
    if (!backend.running) return
    backend.write(JSON.stringify(command) + "\n")
  }
  function persistReceiverEnabled(enabled) {
    receiverEnabled = enabled
    settings = Object.assign({}, settings, { receiverEnabled: enabled })
    if (bar && bar.shell) bar.shell.updateEntryInline(moduleName, settings)
  }
  function toggleReceiver() {
    if (shutdownPending) return
    var enable = !receiverEnabled
    backendRestart.stop()
    if (enable) {
      persistReceiverEnabled(true)
      errorText = ""
      statusText = "Starting receiver…"
    } else {
      stopDiscovery()
      shutdownPending = true
      send({command:"shutdown"})
      receiverShutdownFallback.restart()
    }
  }
  function finishReceiverShutdown() {
    receiverShutdownFallback.stop()
    shutdownPending = false
    persistReceiverEnabled(false)
    backendReady = false
    devices = []
    selectedDevice = null
    viewState = "nearby"
    statusText = "Turned off"
    errorText = ""
  }
  function startDiscovery() { discoveryActive = true; errorText = ""; statusText = devices.length ? "Ready" : "Looking nearby…"; send({command:"discovery_start"}) }
  function forceFullDiscovery() { discoveryActive = true; errorText = ""; statusText = "Scanning local network…"; send({command:"discovery_start",force_full:true}) }
  function stopDiscovery() { discoveryActive = false; send({command:"discovery_stop"}) }
  function chooseDevice(index) { if (index >= 0 && index < devices.length) { selectedDevice = devices[index]; viewState = "target"; selectedIndex = 0 } }
  function goBack() { if (viewState === "target") { viewState = "nearby"; selectedDevice = null; selectedIndex = 0 } else close() }
  function selectFiles() { if (!selectedDevice || picker.running) return; picker.running = true }
  function sendClipboard() { if (!selectedDevice || clipboard.running) return; clipboard.running = true }
  function acceptIncoming() { if (!incoming) return; send({command:"accept",request_id:incoming.requestId}); viewState="receiving"; transferPeer=incoming.sender; transferName=Model.incomingSummary(incoming.files); progress=0 }
  function declineIncoming() { if (!incoming) return; send({command:"decline",request_id:incoming.requestId}); incoming=null; viewState="nearby" }
  function finishOutgoing(terminalState, message, completed) {
    outgoingTransferId = ""
    if (incoming) {
      viewState = Model.viewAfterOutgoing(true, terminalState)
      selectedIndex = 1
      statusText = "Incoming transfer"
      errorText = ""
      progress = 0
      return
    }
    viewState = Model.viewAfterOutgoing(false, terminalState)
    statusText = message
    errorText = terminalState === "error" ? message : ""
    if (completed) progress = 1
  }
  function moveCursor(dx, dy) {
    cursorActive = true
    if (viewState === "nearby") {
      if (dy !== 0) selectedIndex = Math.max(-1, Math.min(devices.length, selectedIndex + dy))
      return
    }
    var count = viewState === "target" ? 2 : (viewState === "incoming" ? 2 : 1)
    if (count > 0 && dy !== 0) selectedIndex = Math.max(0, Math.min(count - 1, selectedIndex + dy))
  }
  function activateCursor() {
    if (viewState === "nearby") selectedIndex < 0 ? toggleReceiver() : (selectedIndex === devices.length ? forceFullDiscovery() : chooseDevice(selectedIndex))
    else if (viewState === "target") selectedIndex === 0 ? selectFiles() : sendClipboard()
    else if (viewState === "incoming") selectedIndex === 0 ? declineIncoming() : acceptIncoming()
    else if (viewState === "sending") send({command:"cancel_outgoing",transfer_id:outgoingTransferId})
    else if (viewState === "success" || viewState === "error") { viewState="nearby"; selectedIndex=0 }
  }
  function handleEvent(event) {
    if (!event || !event.event) return
    if (backendVersionMismatch && event.event !== "ready") return
    if (event.event === "ready") {
      if (!Model.helperVersionMatches(pluginVersion, event.helperVersion)) {
        backendReady=false
        backendVersionMismatch=true
        errorText="Nearby backend version mismatch. Run the Nearby installer again."
        statusText=errorText
        send({command:"shutdown"})
        return
      }
      backendAcceptedThisRun=true; backendReady=true; backendVersionMismatch=false; backendRestart.attempts=0; statusText="Ready to receive"; errorText=""; if(opened&&viewState==="nearby")startDiscovery()
    }
    else if (event.event === "peer_snapshot") { devices=Model.snapshotDevices(event.devices); statusText=devices.length ? "Ready" : "Looking nearby…" }
    else if (event.event === "device") { devices=Model.upsertDevice(devices,event.device); statusText=devices.length ? "Ready" : "Looking nearby…" }
    else if (event.event === "discovery_started") discoveryActive=true
    else if (event.event === "discovery_stopped") discoveryActive=false
    else if (event.event === "incoming_request") {
      incoming=event; incomingText=""; if (viewState!=="sending" && viewState!=="receiving") { viewState="incoming"; selectedIndex=1 }
      Quickshell.execDetached(["notify-send","-a","Nearby","Incoming transfer",String(event.sender)+" wants to send "+Model.incomingSummary(event.files)])
    }
    else if (event.event === "incoming_text") {
      incomingText=String(event.text || ""); transferPeer=String(event.sender || ""); viewState="text"; stopDiscovery()
      Quickshell.execDetached(["notify-send","-a","Nearby","Text received","From "+transferPeer])
    }
    else if (event.event === "incoming_accepted") { if (incoming && incoming.requestId===event.requestId) incoming=null }
    else if (event.event === "incoming_expired") { if (incoming && incoming.requestId===event.requestId) { incoming=null; if(viewState==="incoming"||viewState==="receiving"){viewState="error";errorText="Transfer request expired"} } }
    else if (event.event === "incoming_progress") { if(activeIncomingSession==="")activeIncomingSession=String(event.sessionId); if(activeIncomingSession!==String(event.sessionId))return; if(viewState!=="sending")viewState="receiving"; transferName=String(event.name); transferPeer=String(event.sender); progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "file_received") { if(activeIncomingSession!==""&&activeIncomingSession!==String(event.sessionId))return; activeIncomingSession=String(event.sessionId); lastReceivedPath=String(event.path); transferName=String(event.name); transferPeer=String(event.sender) }
    else if (event.event === "incoming_done") { if(activeIncomingSession!==String(event.sessionId))return; activeIncomingSession=""; if(viewState!=="sending"){viewState="success";statusText="Received";progress=1} }
    else if (event.event === "incoming_cancelled") { if(activeIncomingSession!==""&&activeIncomingSession!==String(event.sessionId))return; activeIncomingSession=""; if(viewState!=="sending"){viewState="error";errorText="Transfer cancelled";statusText=errorText} }
    else if (event.event === "incoming_failed") { if(activeIncomingSession!==""&&activeIncomingSession!==String(event.sessionId))return; activeIncomingSession=""; if(viewState!=="sending"){viewState="error";errorText=String(event.message||"Transfer failed");statusText=errorText} }
    else if (event.event === "incoming_declined") { viewState="nearby"; statusText="Declined" }
    else if (event.event === "outgoing_preparing") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; transferName=String(event.name); transferPeer=String(event.target); progress=0; stopDiscovery() }
    else if (event.event === "outgoing_progress") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "outgoing_done") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("success", "Sent", true) }
    else if (event.event === "outgoing_cancelled") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", "Transfer cancelled", false) }
    else if (event.event === "outgoing_failed") { if(event.transferId && String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", String(event.message||"Transfer failed"), false) }
    else if (event.event === "error") { viewState="error"; errorText=String(event.message||"Transfer failed"); statusText=errorText }
  }

  onOpenedChanged: {
    if (opened) {
      cursorActive=false; selectedIndex=-1; nearbyPhraseIndex=0
      if (viewState === "nearby" && receiverEnabled && backendReady) startDiscovery()
      Qt.callLater(function(){ keyCatcher.forceActiveFocus() })
    } else stopDiscovery()
  }

  Process {
    id: backend
    command: [root.pluginDir + "/bin/omarchy-nearby-helper"]
    running: root.receiverEnabled && root.pluginVersion !== ""
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.handleEvent(Model.parseLine(line)) } }
    stderr: SplitParser { onRead: function(line) { console.warn("nearby backend", line) } }
    onStarted: {
      root.backendAcceptedThisRun=false
      root.backendVersionMismatch=false
    }
    onExited: function(code) {
      root.backendReady=false; root.discoveryActive=false
      if (root.shutdownPending) { root.finishReceiverShutdown(); return }
      if (!root.receiverEnabled) { root.statusText="Turned off"; root.errorText=""; return }
      if (root.backendVersionMismatch) return
      if (!root.backendAcceptedThisRun) {
        root.errorText="Nearby backend could not start. Run the Nearby installer again or build it with ./build.sh."
        root.statusText=root.errorText
        return
      }
      root.errorText=code===0 ? "Receiver stopped" : "Receiver unavailable"
      root.statusText=root.errorText
      if (root.viewState==="sending"||root.viewState==="receiving") { root.viewState="error"; root.errorText="Nearby backend stopped during transfer"; root.statusText=root.errorText }
      if (backendRestart.attempts < 4) { backendRestart.attempts++; backendRestart.interval=Math.min(30000,1000*Math.pow(2,backendRestart.attempts-1)); backendRestart.restart() }
    }
  }
  Timer { id: backendRestart; property int attempts: 0; interval: 1000; repeat: false; onTriggered: if (root.receiverEnabled && !backend.running) backend.running=true }
  Timer { id: receiverShutdownFallback; interval: 250; repeat: false; onTriggered: root.finishReceiverShutdown() }

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

  Process {
    id: picker
    command: ["zenity","--file-selection","--multiple","--separator=\n","--title=Send nearby"]
    running: false
    stdout: StdioCollector { id: pickerOutput; waitForEnd: true }
    onExited: function(code) {
      if (code === 127) { root.viewState="error"; root.errorText="zenity is required to choose files"; root.statusText=root.errorText; return }
      if (code !== 0 || !root.selectedDevice) return
      var paths=String(pickerOutput.text || "").split("\n").filter(function(v){return v.trim()!==""})
      if (paths.length) { root.transferSequence++; root.outgoingTransferId="out-"+Date.now()+"-"+root.transferSequence; root.send({command:"send_files",transfer_id:root.outgoingTransferId,device:root.selectedDevice,paths:paths}) }
    }
  }
  Process {
    id: clipboard
    command: ["wl-paste","--no-newline","--type","text"]
    running: false
    stdout: StdioCollector { id: clipboardOutput; waitForEnd: true }
    onExited: function(code) {
      var text=String(clipboardOutput.text || "")
      if (code===0 && text.trim()!=="" && root.selectedDevice) { root.transferSequence++; root.outgoingTransferId="out-"+Date.now()+"-"+root.transferSequence; root.send({command:"send_text",transfer_id:root.outgoingTransferId,device:root.selectedDevice,text:text}) }
      else { root.viewState="error"; root.errorText=code===127 ? "wl-paste is required to read the clipboard" : "Clipboard is empty" }
    }
  }

  BarIconButton {
    id: button; anchors.fill: parent; bar: root.bar; text: "󰀂"
    active: root.incoming !== null || root.viewState === "receiving" || root.viewState === "sending"
    tooltipText: !root.receiverEnabled ? "Nearby · Turned off" : (root.backendReady ? (root.incoming ? "Incoming transfer" : "Nearby · Ready to receive") : "Nearby · Receiver unavailable")
    onPressed: function(code) { root.toggle() }
  }

  KeyboardPanel {
    id: panel; anchorItem: button; owner: root; bar: root.bar; open: root.opened; focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(360))
    contentHeight: panel.fittedContentHeight(content.implicitHeight, Style.space(560))
    PanelKeyCatcher {
      id: keyCatcher; anchors.fill: parent
      onMoveRequested: function(dx,dy){root.moveCursor(dx,dy)}
      onActivateRequested: root.activateCursor()
      onCloseRequested: root.goBack()
      Column {
        id: content; width: parent.width; spacing: Style.space(12)
        PanelHero {
          id: hero
          title: root.viewState === "target" && root.selectedDevice ? root.selectedDevice.alias : (root.viewState === "incoming" ? "Incoming" : "Nearby")
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
            onHovered: function(on) { if (on) { root.cursorActive=true; root.selectedIndex=-1 } }
            onToggled: root.toggleReceiver()
            PanelToolTip { visible: parent.containsMouse; text: root.receiverEnabled ? "Turn Nearby off" : "Turn Nearby on"; fontFamily:root.fontFamily }
          }
        }
        PanelSeparator { width: parent.width }

        Column {
          visible: root.viewState === "nearby"; width: parent.width; spacing: Style.space(4)
          PanelSectionHeader { text: "NEARBY"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { visible: root.devices.length===0; text: !root.receiverEnabled ? "Nearby is turned off" : (root.discoveryActive ? "Finding devices…" : (root.backendReady ? "No devices nearby" : root.errorText)); color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; topPadding:Style.space(12); bottomPadding:Style.space(12) }
          Repeater {
            model: root.devices
            Button { required property var modelData; required property int index; width:parent.width; leftAlign:true; bordered:false; iconText:Model.iconFor(modelData.deviceType); text:modelData.alias; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===index; onHovered:function(v){if(v){root.cursorActive=true;root.selectedIndex=index}}; onClicked:root.chooseDevice(index) }
          }
          Button { visible:root.receiverEnabled&&root.backendReady; width:parent.width; leftAlign:true; bordered:false; iconText:"󰑐"; text:"Search for new devices"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===root.devices.length; onHovered:function(v){if(v){root.cursorActive=true;root.selectedIndex=root.devices.length}}; onClicked:root.forceFullDiscovery() }
        }

        Column {
          visible: root.viewState === "target"; width:parent.width; spacing:Style.space(6)
          PanelSectionHeader { text:"SEND"; foreground:root.foreground; fontFamily:root.fontFamily }
          Button { width:parent.width; leftAlign:true; iconText:"󰈔"; text:"Send files"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===0; onHovered:function(v){if(v){root.cursorActive=true;root.selectedIndex=0}}; onClicked:root.selectFiles() }
          Button { width:parent.width; leftAlign:true; iconText:"󰅇"; text:"Send clipboard"; foreground:root.foreground; fontFamily:root.fontFamily; hasCursor:root.cursorActive&&root.selectedIndex===1; onHovered:function(v){if(v){root.cursorActive=true;root.selectedIndex=1}}; onClicked:root.sendClipboard() }
        }

        Column {
          visible: root.viewState === "incoming" && root.incoming; width:parent.width; spacing:Style.space(8)
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incoming ? root.incoming.sender+" wants to send" : ""; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incoming ? Model.incomingSummary(root.incoming.files)+" · "+Model.formatBytes(root.incoming.total) : ""; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body; elide:Text.ElideRight }
          Row { width:parent.width; spacing:Style.space(8)
            Button { width:(parent.width-parent.spacing)/2; text:"Decline"; foreground:root.urgent; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===0; onClicked:root.declineIncoming() }
            Button { width:(parent.width-parent.spacing)/2; text:"Accept"; foreground:root.foreground; bordered:true; hasCursor:root.cursorActive&&root.selectedIndex===1; onClicked:root.acceptIncoming() }
          }
        }

        Column {
          visible: root.viewState==="sending"||root.viewState==="receiving"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:root.viewState==="sending" ? "SENDING" : "RECEIVING"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.transferName; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true; elide:Text.ElideMiddle }
          Rectangle { width:parent.width; height:Style.space(4); radius:height/2; color:Qt.darker(root.foreground,2.2); Rectangle { width:parent.width*Math.max(0,Math.min(1,root.progress)); height:parent.height; radius:height/2; color:root.foreground; Behavior on width { NumberAnimation { duration:120 } } } }
          Text { text:Math.round(root.progress*100)+"% · "+(root.viewState==="sending"?"to ":"from ")+root.transferPeer; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { visible:root.viewState==="sending"; text:"Cancel"; bordered:true; foreground:root.urgent; hasCursor:root.cursorActive; onClicked:root.send({command:"cancel_outgoing",transfer_id:root.outgoingTransferId}) }
        }

        Column {
          visible: root.viewState==="success"||root.viewState==="error"; width:parent.width; spacing:Style.space(8)
          Text { text:root.viewState==="success" ? root.statusText : root.errorText; color:root.viewState==="error"?root.urgent:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.title; font.bold:true }
          Text { visible:root.viewState==="success"; text:root.transferPeer!=="" ? (root.statusText==="Sent"?"to ":"from ")+root.transferPeer : ""; color:root.dim; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { text:"Done"; bordered:true; foreground:root.foreground; hasCursor:root.cursorActive; onClicked:{root.viewState="nearby";root.startDiscovery()} }
        }

        Column {
          visible: root.viewState==="text"; width:parent.width; spacing:Style.space(8)
          PanelSectionHeader { text:"RECEIVED TEXT"; foreground:root.foreground; fontFamily:root.fontFamily }
          Text { width:parent.width; textFormat:Text.PlainText; text:root.incomingText; wrapMode:Text.Wrap; maximumLineCount:6; elide:Text.ElideRight; color:root.foreground; font.family:root.fontFamily; font.pixelSize:Style.font.body }
          Button { text:"Copy"; iconText:"󰆏"; bordered:true; foreground:root.foreground; onClicked:{clipboardWriter.running=true} }
        }
      }
    }
  }

  Process {
    id: clipboardWriter
    command: ["wl-copy"]
    running: false
    stdinEnabled: true
    onStarted: { write(root.incomingText); closeStdin() }
    onExited: function(code) { if(code===0){root.viewState="success";root.statusText="Received"} else {root.viewState="error";root.errorText="wl-copy is required to copy received text"} }
  }
}
