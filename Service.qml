import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model

// One Nearby engine per shell session, not one per monitor.
//
// The bar instantiates its widgets once per screen (Variants over
// Quickshell.screens), so a helper owned by the widget was started once per
// monitor. The helper binds the LocalSend port, so on a second monitor every
// instance after the first lost the race and exited with EADDRINUSE, leaving
// that bar reporting a receiver that could not start while another copy held
// the port. Registering the IPC target that many times had the same problem:
// only the first registration was used.
//
// The shell loads a `service` kind exactly once, so the helper, the transfer
// state, and the IPC target live here. Every bar widget is a view onto this
// object and owns nothing but its own cursor and popup.
Item {
  id: root

  // Injected by the shell when the service is created.
  property var shell: null
  property var manifest: null
  property string omarchyPath: Quickshell.env("OMARCHY_PATH") || ""

  readonly property string manifestPluginId: "oma.nearby"
  readonly property string metadataSourceDir: manifest && manifest.__sourceDir
    ? String(manifest.__sourceDir)
    : ""
  readonly property string pluginDir: metadataSourceDir !== ""
    ? metadataSourceDir
    : (Quickshell.env("HOME") || "") + "/.config/omarchy/plugins/" + manifestPluginId

  // Settings come from shell.json, not from the widgets.
  //
  // A widget cannot be the source here. The bar builds one per monitor and
  // injects `settings` a tick after the widget is created, so the first thing
  // a widget can report is the default rather than the persisted value, and
  // the rest are copies of the same entry. Reading the entry directly means
  // the persisted state and the effective state are the same value, so they
  // cannot drift apart, and `receiverConfigured` is a fact about the config
  // rather than about which widget spoke first.
  readonly property var configEntry: shell && shell.shellConfig
    ? Model.barEntry(shell.shellConfig, manifestPluginId)
    : null
  readonly property bool receiverConfigured: configEntry !== null
  readonly property string moduleEntryId: configEntry ? configEntry.id : manifestPluginId
  readonly property var pluginSettings: configEntry ? configEntry.settings : ({})
  readonly property bool receiverEnabled: Model.receiverEnabledIn(configEntry)

  property string pluginVersion: ""
  property bool backendReady: false
  property bool discoveryActive: false
  property var devices: []
  property var selectedDevice: null
  property string viewState: "nearby"
  property string statusText: "Starting receiver…"
  property string errorText: ""
  property var incomingQueue: []
  readonly property var incoming: Model.currentIncoming(incomingQueue)
  property string incomingText: ""
  property bool incomingTextPending: false
  property string lastReceivedPath: ""
  property real progress: 0
  property string transferName: ""
  property string transferPeer: ""
  property string activeIncomingSession: ""
  property string outgoingTransferId: ""
  property var pendingOutgoing: null
  property string pinError: ""
  property int transferSequence: 0
  property bool shutdownPending: false
  property bool backendAcceptedThisRun: false
  property bool backendVersionMismatch: false
  property string backendStartupFailureCode: ""
  property int backendStartupFailurePort: 0

  // Discovery follows the popup, and the popup exists once per monitor, so
  // openness is a count rather than a flag: discovery runs while any view is
  // open and stops when the last one closes.
  property int openViewCount: 0
  readonly property bool anyViewOpen: openViewCount > 0
  readonly property bool backendRunning: backend.running

  // Views own their cursor and their PIN field; the engine only asks. A single
  // engine driving several popups cannot reach into any one of them.
  signal cursorRequested(int index)
  signal pinCleared()
  signal pinFocusRequested()
  signal focusRestoreRequested()

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

  // Registered once, from the one object there is one of. Registering this
  // from the widget meant one handler per monitor, of which the shell kept the
  // first and warned about the rest.
  //
  // Opening a popup is still the widget's job, so those calls are handed back
  // to the shell, which routes them through the bar to the focused monitor
  // rather than to whichever instance had claimed the target.
  function summonView(action) {
    if (!shell || typeof shell[action] !== "function") return "unknown"
    return shell[action](manifestPluginId, "{}") === false ? "unknown" : "ok"
  }

  IpcHandler {
    target: "oma.nearby"
    function open(): string { return root.summonView("summon") }
    function close(): string { return root.summonView("hide") }
    function toggle(): string { return root.summonView("toggle") }
    function receiverOn(): string { if (!root.receiverEnabled) root.toggleReceiver(); return "ok" }
    function receiverOff(): string { if (root.receiverEnabled) root.toggleReceiver(); return "ok" }
    function receiverToggle(): string { root.toggleReceiver(); return "ok" }
    function status(): string { return JSON.stringify({enabled:root.receiverEnabled,ready:root.backendReady,running:backend.running,devices:root.devices.length}) }
  }

  function viewOpened() {
    openViewCount++
    if (openViewCount !== 1) return
    if (viewState === "nearby" && receiverEnabled && backendReady) startDiscovery()
  }
  function viewClosed() {
    if (openViewCount > 0) openViewCount--
    if (openViewCount === 0) stopDiscovery()
  }

  function send(command) {
    if (!backend.running) return
    backend.write(JSON.stringify(command) + "\n")
  }
  function bindBackendRunning() {
    // Assigning a plain `true` here removes the declarative binding from
    // Process.running. Reinstall the binding instead so receiver OFF -> ON can
    // still start the helper after the retry budget has been exhausted.
    backend.running = Qt.binding(function() {
      return root.receiverConfigured && root.receiverEnabled && root.pluginVersion !== ""
    })
  }
  // receiverEnabled follows the entry, so writing the entry is what turns the
  // receiver on or off. There is no second copy of the state to keep in step.
  function persistReceiverEnabled(enabled) {
    if (!shell) return
    var settings = Object.assign({}, pluginSettings, { receiverEnabled: enabled })
    if (Model.hasStringBarEntry(shell.shellConfig, moduleEntryId)
        && typeof shell.mutateShellConfig === "function") {
      shell.mutateShellConfig(function(config) {
        Model.promoteStringBarEntry(config, moduleEntryId, settings)
      })
      return
    }
    shell.updateEntryInline(moduleEntryId, settings)
  }
  function toggleReceiver() {
    if (shutdownPending) return
    var enable = !receiverEnabled
    backendRestart.stop()
    if (enable) {
      backendRestart.attempts = 0
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
    incomingQueue = []
    incomingTextPending = false
    activeIncomingSession = ""
    clearPendingOutgoing()
    viewState = "nearby"
    statusText = "Turned off"
    errorText = ""
  }
  function startDiscovery() { discoveryActive = true; errorText = ""; statusText = devices.length ? "Ready" : "Looking nearby…"; send({command:"discovery_start"}) }
  function forceFullDiscovery() { discoveryActive = true; errorText = ""; statusText = "Scanning local network…"; send({command:"discovery_start",force_full:true}) }
  function stopDiscovery() { discoveryActive = false; send({command:"discovery_stop"}) }
  function chooseDevice(index) { if (index >= 0 && index < devices.length) { selectedDevice = devices[index]; viewState = "target"; cursorRequested(0) } }
  function clearTarget() { viewState = "nearby"; selectedDevice = null; cursorRequested(0) }
  function failWith(message) { viewState="error"; errorText=message; statusText=errorText }
  function cancelOutgoing() { send({command:"cancel_outgoing",transfer_id:outgoingTransferId}) }
  function noteTextCopied() { if (viewState !== "text") return; viewState="success"; statusText="Received" }
  function clearPendingOutgoing() { pendingOutgoing=null; outgoingTransferId=""; pinError=""; pinCleared() }
  function dispatchPendingOutgoing(pin) {
    if (!pendingOutgoing) return
    transferSequence++
    outgoingTransferId="out-"+Date.now()+"-"+transferSequence
    var command=Model.outgoingCommand(pendingOutgoing,outgoingTransferId,pin)
    if (!command) { clearPendingOutgoing(); viewState="error"; errorText="Transfer failed"; return }
    pinCleared()
    pinError=""
    send(command)
  }
  function beginOutgoing(pending) { if(pendingOutgoing||!backend.running)return; pendingOutgoing=pending; dispatchPendingOutgoing(null) }
  function retryWithPin(pin) {
    if (outgoingTransferId !== "") return
    var entered=String(pin || "")
    if (entered === "") { pinError="Enter the receiver PIN"; pinFocusRequested(); return }
    dispatchPendingOutgoing(entered)
  }
  function showPinPrompt(message) {
    outgoingTransferId=""
    viewState="pin"
    pinError=message
    statusText="PIN required"
    pinFocusRequested()
  }
  function cancelPin() { if(outgoingTransferId!=="")send({command:"cancel_outgoing",transfer_id:outgoingTransferId}); clearPendingOutgoing(); if(incoming){viewState="incoming";cursorRequested(1)}else if(incomingTextPending){incomingTextPending=false;viewState="text";statusText="Text received";cursorRequested(0)}else{viewState=selectedDevice?"target":"nearby";cursorRequested(0)} focusRestoreRequested() }
  function acceptIncoming() { if (!incoming) return; send({command:"accept",request_id:incoming.requestId}); viewState="receiving"; transferPeer=incoming.sender; transferName=Model.incomingSummary(incoming.files); progress=0 }
  function declineIncoming() {
    if (!incoming) return
    var requestId = incoming.requestId
    send({command:"decline",request_id:requestId})
    incomingQueue = Model.removeIncoming(incomingQueue, requestId)
    if (!incoming&&incomingTextPending) { incomingTextPending=false; viewState="text"; statusText="Text received" }
    else { viewState = incoming ? "incoming" : "nearby"; statusText = incoming ? "Incoming transfer" : "Declined" }
    cursorRequested(incoming ? 1 : 0)
  }
  function finishIncoming(terminalState, message, completed) {
    activeIncomingSession = ""
    if (pendingOutgoing) return
    if (incoming) {
      viewState = "incoming"
      cursorRequested(1)
      statusText = "Incoming transfer"
      errorText = ""
      progress = 0
      return
    }
    if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText="Text received"; errorText=""; progress=0; return }
    viewState = terminalState
    statusText = message
    errorText = terminalState === "error" ? message : ""
    if (completed) progress = 1
  }
  function finishOutgoing(terminalState, message, completed) {
    clearPendingOutgoing()
    if (incoming) {
      viewState = Model.viewAfterOutgoing(true, terminalState)
      cursorRequested(1)
      statusText = "Incoming transfer"
      errorText = ""
      progress = 0
      return
    }
    if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText="Text received"; errorText=""; progress=0; return }
    viewState = Model.viewAfterOutgoing(false, terminalState)
    statusText = message
    errorText = terminalState === "error" ? message : ""
    if (completed) progress = 1
  }
  function finishText() { incomingText=""; incomingTextPending=false; viewState="nearby"; cursorRequested(0); startDiscovery() }
  function finishTerminal() { viewState="nearby"; cursorRequested(0); startDiscovery() }
  function handleBackendExit(code) {
    backendReady=false; discoveryActive=false; incomingQueue=[]; incomingTextPending=false; activeIncomingSession=""; clearPendingOutgoing()
    if (shutdownPending) { finishReceiverShutdown(); return }
    if (!receiverEnabled) { statusText="Turned off"; errorText=""; return }
    if (backendVersionMismatch) return
    if (!backendAcceptedThisRun) {
      // An early exit alone does not identify its cause. The helper reports a
      // confirmed bind conflict as a structured event before it exits; all
      // other pre-ready failures remain generic. Both keep the existing
      // bounded retry schedule so a transient conflict can recover.
      if (backendRestart.attempts < 4) {
        errorText = "Nearby receiver could not start. Retrying…"
      } else if (backendStartupFailureCode === "receiver_port_in_use") {
        var port = backendStartupFailurePort > 0 ? backendStartupFailurePort : 53317
        errorText = "LocalSend receiver port " + port + " is already in use. Another LocalSend receiver on this computer may be running. Quit it and enable Nearby again."
      } else {
        errorText = "Nearby receiver could not start."
      }
      statusText=errorText
    } else {
      errorText=code===0 ? "Receiver stopped" : "Receiver unavailable"
      statusText=errorText
      if (viewState==="sending"||viewState==="receiving"||viewState==="pin"||viewState==="incoming") { viewState="error"; errorText="Nearby backend stopped during transfer"; statusText=errorText }
    }
    if (backendRestart.attempts < 4) { backendRestart.attempts++; backendRestart.interval=Math.min(30000,1000*Math.pow(2,backendRestart.attempts-1)); backendRestart.restart() }
  }
  function handleEvent(event) {
    if (!event || !event.event) return
    if (backendVersionMismatch && event.event !== "ready") return
    if (event.event === "startup_failed") {
      backendStartupFailureCode=String(event.code || "")
      backendStartupFailurePort=Number(event.port) || 0
    }
    else if (event.event === "ready") {
      if (!Model.helperVersionMatches(pluginVersion, event.helperVersion)) {
        backendReady=false
        backendVersionMismatch=true
        errorText="Nearby backend version mismatch. Run the Nearby installer again."
        statusText=errorText
        send({command:"shutdown"})
        return
      }
      backendAcceptedThisRun=true; backendReady=true; backendVersionMismatch=false; backendRestart.attempts=0; statusText="Ready to receive"; errorText=""; if(anyViewOpen&&viewState==="nearby")startDiscovery()
    }
    else if (event.event === "peer_snapshot") { devices=Model.snapshotDevices(event.devices); statusText=devices.length ? "Ready" : "Looking nearby…" }
    else if (event.event === "device") { devices=Model.upsertDevice(devices,event.device); statusText=devices.length ? "Ready" : "Looking nearby…" }
    else if (event.event === "discovery_started") discoveryActive=true
    else if (event.event === "discovery_stopped") discoveryActive=false
    else if (event.event === "incoming_request") {
      incomingQueue=Model.enqueueIncoming(incomingQueue,event); if(!incomingTextPending)incomingText=""; if (viewState!=="sending" && viewState!=="receiving" && viewState!=="pin") { viewState="incoming"; cursorRequested(1) }
      Quickshell.execDetached(["notify-send","-a","Nearby","Incoming transfer",String(event.sender)+" wants to send "+Model.incomingSummary(event.files)])
    }
    else if (event.event === "incoming_text") {
      incomingText=String(event.text || ""); transferPeer=String(event.sender || ""); incomingTextPending=pendingOutgoing!==null; if (!pendingOutgoing) { viewState="text"; stopDiscovery() }
      Quickshell.execDetached(["notify-send","-a","Nearby","Text received","From "+String(event.sender || "")])
    }
    else if (event.event === "incoming_accepted") { incomingQueue=Model.removeIncoming(incomingQueue,event.requestId) }
    else if (event.event === "incoming_expired") {
      var expiredWasCurrent=incoming&&incoming.requestId===event.requestId
      incomingQueue=Model.removeIncoming(incomingQueue,event.requestId)
      if (expiredWasCurrent&&(viewState==="incoming"||(viewState==="receiving"&&activeIncomingSession===""))) {
        if (incoming) { statusText="Incoming transfer"; cursorRequested(1) }
        else if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText="Text received"; errorText="" }
        else { viewState="error"; errorText="Transfer request expired"; statusText=errorText }
      }
    }
    else if (event.event === "incoming_progress") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; if(pendingOutgoing)return; viewState="receiving"; transferName=String(event.name); transferPeer=String(event.sender); progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "file_received") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; lastReceivedPath=String(event.path); if(pendingOutgoing)return; transferName=String(event.name); transferPeer=String(event.sender) }
    else if (event.event === "incoming_done") { if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("success","Received",true) }
    else if (event.event === "incoming_cancelled") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("error","Transfer cancelled",false) }
    else if (event.event === "incoming_failed") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("error",String(event.message||"Transfer failed"),false) }
    else if (event.event === "incoming_declined") { incomingQueue=Model.removeIncoming(incomingQueue,event.requestId) }
    else if (event.event === "outgoing_preparing") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; transferName=String(event.name); transferPeer=String(event.target); progress=0; stopDiscovery() }
    else if (event.event === "outgoing_progress") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "outgoing_done") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("success", "Sent", true) }
    else if (event.event === "outgoing_cancelled") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", "Transfer cancelled", false) }
    else if (event.event === "outgoing_pin_required") { if(String(event.transferId)!==outgoingTransferId)return; showPinPrompt("") }
    else if (event.event === "outgoing_invalid_pin") { if(String(event.transferId)!==outgoingTransferId)return; showPinPrompt("Incorrect PIN") }
    else if (event.event === "outgoing_failed") { if(event.transferId && String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", String(event.message||"Transfer failed"), false) }
    else if (event.event === "error") { viewState="error"; errorText=String(event.message||"Transfer failed"); statusText=errorText }
  }

  Process {
    id: backend
    property bool launched: false
    command: [root.pluginDir + "/bin/omarchy-nearby-helper"]
    // Not eligible to start until shell.json has been read. Defaulting to on
    // before then would let the helper bind the LocalSend port and announce
    // itself on the network for a user who had turned the receiver off.
    running: root.receiverConfigured && root.receiverEnabled && root.pluginVersion !== ""
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.handleEvent(Model.parseLine(line)) } }
    stderr: SplitParser { onRead: function(line) { console.warn("nearby backend", line) } }
    onStarted: {
      backend.launched=true
      root.backendAcceptedThisRun=false
      root.backendVersionMismatch=false
      root.backendStartupFailureCode=""
      root.backendStartupFailurePort=0
    }
    // A helper that is missing rather than failing never reaches onExited:
    // Quickshell returns running to false without an exit code, the same way
    // the file chooser and clipboard readers report a command that never ran.
    // This is the case reinstalling actually fixes.
    onRunningChanged: {
      if (running) { backend.launched=false; return }
      if (backend.launched) return
      // `running` is a binding, so it also drops when the receiver is turned
      // off or the manifest version goes away. Only a helper we still want
      // running counts as one that failed to launch.
      if (!root.receiverEnabled || root.pluginVersion === "") return
      root.backendReady=false
      root.errorText="Nearby helper is missing. Run the Nearby installer again or build it with ./build.sh."
      root.statusText=root.errorText
    }
    onExited: function(code) { root.handleBackendExit(code) }
  }
  Timer { id: backendRestart; property int attempts: 0; interval: 1000; repeat: false; onTriggered: if (root.receiverEnabled && !backend.running) root.bindBackendRunning() }
  Timer { id: receiverShutdownFallback; interval: 250; repeat: false; onTriggered: root.finishReceiverShutdown() }
}
