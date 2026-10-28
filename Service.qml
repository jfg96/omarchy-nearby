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
  // Omarchy's third-party plugin security boundary (4.0.3+) injects a scoped
  // PluginShellApi that no longer carries shellConfig. Legacy hosts still do.
  // The service watches shell.json directly so the persisted state keeps
  // arriving; the injected config stays authoritative when a host provides it.
  property var fileShellConfig: null
  readonly property bool hasLegacyShellConfig: !!shell && !!shell.shellConfig
  readonly property var effectiveShellConfig: hasLegacyShellConfig ? shell.shellConfig : fileShellConfig
  readonly property var configEntry: effectiveShellConfig
    ? Model.barEntry(effectiveShellConfig, manifestPluginId)
    : null
  readonly property bool receiverConfigured: configEntry !== null
  readonly property string moduleEntryId: configEntry ? configEntry.id : manifestPluginId
  readonly property var pluginSettings: configEntry ? configEntry.settings : ({})
  readonly property bool receiverEnabled: Model.receiverEnabledIn(configEntry)
  // The folder for received files is a setting of the entry too. The helper
  // resolves its download directory once, at startup, and already honours
  // XDG_DOWNLOAD_DIR, so a chosen folder is handed to it as that variable and
  // takes effect by restarting it. Empty leaves the XDG lookup as it was.
  readonly property string downloadDir: Model.downloadDirIn(pluginSettings)
  onDownloadDirChanged: restartBackend()
  // What the running helper resolved, which is what the panel shows: it is
  // the effective location whether or not a folder was chosen.
  property string saveDirectory: ""
  property bool restartPending: false
  readonly property string homeDir: Quickshell.env("HOME") || ""

  property string pluginVersion: ""
  property string minHelperVersion: ""
  // The version the helper has to reach. A manifest that does not declare a
  // floor is read as requiring the version it ships with, which is the rule
  // this plugin followed before the field existed.
  readonly property string requiredHelperVersion: minHelperVersion !== "" ? minHelperVersion : pluginVersion
  property string helperVersion: ""
  property bool backendReady: false
  property bool discoveryActive: false
  property var devices: []
  property var selectedDevice: null
  property string viewState: "nearby"
  property string statusText: strings.t("status.startingReceiver")
  property bool transferWasOutgoing: false
  property string errorText: ""
  property var incomingQueue: []
  readonly property var incoming: Model.currentIncoming(incomingQueue)
  property string incomingText: ""
  property bool incomingTextPending: false
  property string lastReceivedPath: ""
  // The one file the receive on screen delivered, offered for preview and
  // copy. A session of several files keeps the plain result.
  property string receivedSession: ""
  property int receivedFileCount: 0
  property string receivedFile: ""
  property string receivedFileType: ""
  // What the result shows of a received picture: the file itself, or a PNG
  // converted from a format the shell cannot decode.
  property string receivedPreview: ""
  onViewStateChanged: if (viewState !== "success") { receivedFile = ""; receivedFileType = ""; receivedPreview = "" }
  property real progress: 0
  property string transferName: ""
  property string transferPeer: ""
  property string activeIncomingSession: ""
  property string outgoingTransferId: ""
  property var pendingOutgoing: null
  property string pinError: ""
  property bool incomingPinEnabled: false
  property bool incomingPinUpdating: false
  property var pendingIncomingPinEnabled: null
  property string incomingPinError: ""
  property int transferSequence: 0
  property bool shutdownPending: false
  property bool backendAcceptedThisRun: false
  property bool backendVersionMismatch: false
  property string backendStartupFailureCode: ""
  property int backendStartupFailurePort: 0

  // The checkout selects immutable helper bytes in helper-release.env. The
  // launcher keeps those bytes outside the watched plugin tree and can repair
  // a missing or incompatible cached helper through the action below.
  property bool helperMissing: false
  property bool helperUpdating: false
  property bool helperUpdateInstalled: false
  property string helperUpdateStatus: ""
  property string helperUpdateError: ""
  // Plugin and helper versions have independent release cycles. A compatible
  // helper is not "behind" merely because the plugin version is higher.
  readonly property bool helperUpdateOffered: pluginVersion !== ""
    && (backendVersionMismatch || helperMissing)
  readonly property int maxOutgoingTextBytes: 1024 * 1024
  // One line, and the only place the versions are stated. The hero shows the
  // short status and the panel shows this; saying it in both is what made the
  // popup repeat itself three times over four lines.
  readonly property string helperUpdateDetail: !helperUpdateOffered
    ? ""
    : (helperMissing
      ? strings.t("helper.missing")
      : strings.t("helper.versions", [requiredHelperVersion, helperVersion !== "" ? helperVersion : strings.t("helper.versionUnknown")]))

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
  signal incomingPinCleared()
  signal incomingPinFocusRequested()
  signal focusRestoreRequested()

  Strings { id: strings }

  FileView {
    path: root.pluginDir + "/manifest.json"
    printErrors: false
    onLoaded: {
      root.pluginVersion = Model.manifestVersion(text(), root.manifestPluginId)
      root.minHelperVersion = Model.manifestMinHelperVersion(text(), root.manifestPluginId)
      if (root.pluginVersion === "") {
        root.errorText = strings.t("status.manifestUnavailable")
        root.statusText = root.errorText
      }
    }
    onLoadFailed: function(error) {
      root.pluginVersion = ""
      root.minHelperVersion = ""
      root.errorText = strings.t("status.manifestUnavailable")
      root.statusText = root.errorText
    }
  }

  // The scoped plugin shell no longer exposes shellConfig on modern Omarchy, so
  // the persisted state is read from the shell.json Omarchy owns. Read-only:
  // every write still goes back through the shell API, and watching the file
  // also covers the shell's own atomic rewrite after it applies an update.
  FileView {
    path: (Quickshell.env("HOME") || "") + "/.config/omarchy/shell.json"
    watchChanges: true
    printErrors: false
    onLoaded: {
      root.fileShellConfig = Model.parseShellConfig(text())
    }
    onLoadFailed: function(error) {
      root.fileShellConfig = null
    }
    onFileChanged: reload()
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
    // The same repair the panel button runs, for a shell whose bar is not
    // where the user is looking when the helper stops matching.
    function retryHelper(): string { if (root.helperUpdating) return "busy"; root.startHelperUpdate(); return "ok" }
    // The helper version and the floor it has to clear are reported here
    // because a mismatch otherwise shows up only as a panel that says it is
    // not ready, which is the same thing a port conflict looks like.
    function status(): string { return JSON.stringify({enabled:root.receiverEnabled,ready:root.backendReady,running:backend.running,devices:root.devices.length,helper:root.helperVersion,requires:root.requiredHelperVersion,updateOffered:root.helperUpdateOffered}) }
    // The click action of an incoming-transfer notification.
    function showRequest(requestId: string): string { return root.showIncomingRequest(requestId) }
  }

  // Opens the panel on a request a notification announced, never answering it.
  // A request that has expired or been answered since then opens nothing, so a
  // late click or one from the notification history has no effect.
  function showIncomingRequest(requestId) {
    if (!Model.hasIncoming(incomingQueue, requestId)) return "expired"
    return summonView("summon")
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

  // A desktop notification exists to reach a user who is not looking at the
  // panel. Raising one for something the open panel is already showing is a
  // duplicate the user has to dismiss on top of the prompt they are answering.
  //
  // Open is not enough on its own: an arrival during a transfer, a PIN prompt,
  // or behind an earlier queued request is held back rather than displayed, so
  // the panel being open would otherwise swallow it silently. The caller says
  // whether this particular event reached the screen.
  function notificationNeeded(onScreen) {
    return !anyViewOpen || !onScreen
  }

  function send(command) {
    if (!backend.running) return
    backend.write(JSON.stringify(command) + "\n")
  }
  function utf8ByteLength(value) {
    var text=String(value || "")
    var bytes=0
    for (var index=0; index<text.length; index++) {
      var code=text.charCodeAt(index)
      if (code<=0x7f) bytes++
      else if (code<=0x7ff) bytes+=2
      else if (code>=0xd800 && code<=0xdbff && index+1<text.length
               && text.charCodeAt(index+1)>=0xdc00 && text.charCodeAt(index+1)<=0xdfff) {
        bytes+=4
        index++
      } else bytes+=3
    }
    return bytes
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
  // Modern Omarchy exposes only the self-scoped updateEntryInline(), which
  // updates Nearby's own entry. The string-form promotion needs a real host
  // config object, so it stays confined to legacy hosts that still inject
  // shell.shellConfig: on a scoped shell a mutateShellConfig() that exists but
  // is denied must not capture the toggle and drop the write.
  function persistReceiverEnabled(enabled) { persistSettings({ receiverEnabled: enabled }) }
  function persistSettings(changes) {
    if (!shell) return
    var settings = Object.assign({}, pluginSettings, changes)
    // A change to undefined removes the setting from the entry.
    for (var key in settings) if (settings[key] === undefined) delete settings[key]
    if (hasLegacyShellConfig
        && Model.hasStringBarEntry(effectiveShellConfig, moduleEntryId)
        && typeof shell.mutateShellConfig === "function") {
      shell.mutateShellConfig(function(config) {
        Model.promoteStringBarEntry(config, moduleEntryId, settings)
      })
      return
    }
    shell.updateEntryInline(moduleEntryId, settings)
  }
  // A chooser opened before a transfer must not change settings and restart
  // the helper once that transfer or an approval/security request is active.
  function downloadDirChangeBusy() { return pendingOutgoing || activeIncomingSession !== "" || incoming || incomingPinUpdating || viewState === "incoming" || viewState === "receiving" || viewState === "sending" || viewState === "pin" }
  function setDownloadDir(path) {
    if (downloadDirChangeBusy()) return
    if (Model.downloadDirIn({ downloadDir: path }) === "" || path === downloadDir) return
    if (Model.containsHome(path, homeDir)) {
      // Said only while the list the chooser was opened from is still shown,
      // so it cannot take the place of a request that arrived meanwhile.
      if (viewState === "nearby") {
        viewState = "error"
        reportFailure(strings.t("folder.notUsed"), strings.t("folder.notUsedDetail"))
      }
      return
    }
    persistSettings({ downloadDir: path })
  }
  // Going back to Downloads removes the setting rather than naming a folder,
  // so the location follows user-dirs.dirs again.
  function useDownloads() { if (!downloadDirChangeBusy() && downloadDir !== "") persistSettings({ downloadDir: undefined }) }
  // A stopped helper is started again by the same bounded restart that covers
  // one that exits on its own, so asking it to stop is all a restart takes. It
  // then starts with the environment as it is now.
  function restartBackend() {
    if (shutdownPending) return
    if (backend.running) { restartPending = true; send({command:"shutdown"}); return }
    // A receiver that gave up, as it does when the previous folder could not
    // be created, gets its attempts back for the new one. The timer starts it
    // on the next turn, once the environment of the helper follows the setting.
    if (!receiverEnabled || backendVersionMismatch || helperMissing) return
    backendRestart.attempts = 0
    backendRestart.interval = 0
    backendRestart.restart()
    reportFailure(strings.t("status.startingReceiver"), "")
  }
  function toggleReceiver() {
    if (shutdownPending) return
    var enable = !receiverEnabled
    backendRestart.stop()
    if (enable) {
      backendRestart.attempts = 0
      persistReceiverEnabled(true)
      errorText = ""
      statusText = strings.t("status.startingReceiver")
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
    statusText = strings.t("status.turnedOff")
    errorText = ""
  }
  function startDiscovery() { discoveryActive = true; errorText = ""; statusText = devices.length ? strings.t("status.ready") : strings.t("status.lookingNearby"); send({command:"discovery_start"}) }
  function forceFullDiscovery() { discoveryActive = true; errorText = ""; statusText = strings.t("status.scanning"); send({command:"discovery_start",force_full:true}) }
  function stopDiscovery() { discoveryActive = false; send({command:"discovery_stop"}) }
  function chooseDevice(index) { if (index >= 0 && index < devices.length) { selectedDevice = devices[index]; viewState = "target"; cursorRequested(0) } }
  function clearTarget() { viewState = "nearby"; selectedDevice = null; cursorRequested(0) }
  function openIncomingPinSettings() { if(!backendReady)return; incomingPinError=""; viewState="incoming_pin_settings"; cursorRequested(0) }
  function beginIncomingPinEdit() { if(incomingPinUpdating||!backendReady)return; incomingPinError=""; viewState="incoming_pin_edit"; incomingPinCleared(); incomingPinFocusRequested() }
  function requestDisableIncomingPin() { if(incomingPinUpdating||!backendReady||!incomingPinEnabled)return; incomingPinError=""; viewState="incoming_pin_disable"; cursorRequested(0) }
  function cancelIncomingPinSettings() {
    if(incomingPinUpdating)return
    incomingPinError=""; incomingPinCleared()
    if(viewState==="incoming_pin_edit"||viewState==="incoming_pin_disable"){viewState="incoming_pin_settings";cursorRequested(0)}
    else{viewState="nearby";cursorRequested(0);focusRestoreRequested()}
  }
  function submitIncomingPin(pin) {
    if(incomingPinUpdating||!backendReady)return
    var entered=String(pin || "")
    if(!/^[A-Za-z0-9._~-]{1,64}$/.test(entered)){incomingPinError=strings.t("pin.rules");incomingPinFocusRequested();return}
    incomingPinUpdating=true; pendingIncomingPinEnabled=true; incomingPinError=""
    send({command:"set_incoming_pin",pin:entered})
  }
  function confirmDisableIncomingPin() {
    if(incomingPinUpdating||!backendReady||!incomingPinEnabled)return
    incomingPinUpdating=true; pendingIncomingPinEnabled=false; incomingPinError=""
    send({command:"disable_incoming_pin"})
  }
  // Source updates and helper releases are independent. The repair command asks the
  // launcher to prefetch the immutable helper selected by this checkout.
  function startHelperUpdate() {
    if (helperUpdating || pluginVersion === "" || helperUpdater.running) return
    helperUpdateError=""
    helperUpdateStatus=strings.t("helper.contacting")
    helperUpdateInstalled=false
    helperUpdating=true
    helperUpdater.launched=false
    helperUpdater.running=true
  }
  function handleUpdaterEvent(event) {
    if (!event || !event.event) return
    if (event.event === "step") helperUpdateStatus=String(event.message || "")
    else if (event.event === "done") { helperUpdateStatus=""; helperUpdateError=""; helperUpdateInstalled=true }
    else if (event.event === "failed") { helperUpdateStatus=""; helperUpdateError=String(event.message || "") }
  }
  function finishHelperUpdate(code) {
    helperUpdating=false
    helperUpdateStatus=""
    // The repair command reports success only after the cached helper verifies.
    if (code !== 0 && !helperUpdateInstalled) {
      if (helperUpdateError === "") helperUpdateError=strings.t("helper.updateFailed")
      return
    }
    helperUpdateError=""
    helperMissing=false
    helperVersion=""
    backendVersionMismatch=false
    backendStartupFailureCode=""
    backendStartupFailurePort=0
    backendRestart.stop()
    backendRestart.attempts=0
    errorText=""
    statusText=strings.t("status.startingReceiver")
    // The mismatch shutdown left Process.running written rather than bound, so
    // the binding has to be restored before the new helper can start. Same
    // reason backendRestart calls this instead of assigning true.
    if (!backend.running) bindBackendRunning()
  }
  // The hero gets a short status and the body gets the detail. Assigning one
  // sentence to both is what cropped it: the hero line is uppercased and
  // letterspaced, so anything longer than a few words ends in an ellipsis
  // partway through a word and tells the user less than the icon already did.
  function reportFailure(summary, detail) { statusText=summary; errorText=detail }
  function failWith(message) { viewState="error"; errorText=message; statusText=errorText }
  function cancelOutgoing() { send({command:"cancel_outgoing",transfer_id:outgoingTransferId}) }
  function noteTextCopied() { if (viewState !== "text") return; viewState="success"; transferWasOutgoing=false; statusText=strings.t("status.received") }
  function clearPendingOutgoing() { pendingOutgoing=null; outgoingTransferId=""; pinError=""; pinCleared() }
  function dispatchPendingOutgoing(pin) {
    if (!pendingOutgoing) return
    transferSequence++
    outgoingTransferId="out-"+Date.now()+"-"+transferSequence
    var command=Model.outgoingCommand(pendingOutgoing,outgoingTransferId,pin)
    if (!command) { clearPendingOutgoing(); viewState="error"; errorText=strings.t("status.transferFailed"); return }
    pinCleared()
    pinError=""
    send(command)
  }
  function beginOutgoing(pending) {
    if(pendingOutgoing||!backend.running)return
    if(pending && pending.kind==="text" && utf8ByteLength(pending.text)>maxOutgoingTextBytes) {
      failWith(strings.t("status.textTooLarge"))
      return
    }
    pendingOutgoing=pending
    dispatchPendingOutgoing(null)
  }
  function retryWithPin(pin) {
    if (outgoingTransferId !== "") return
    var entered=String(pin || "")
    if (entered === "") { pinError=strings.t("pin.enterReceiverPin"); pinFocusRequested(); return }
    dispatchPendingOutgoing(entered)
  }
  function showPinPrompt(message) {
    outgoingTransferId=""
    viewState="pin"
    pinError=message
    statusText=strings.t("status.pinRequired")
    pinFocusRequested()
  }
  function cancelPin() { if(outgoingTransferId!=="")send({command:"cancel_outgoing",transfer_id:outgoingTransferId}); clearPendingOutgoing(); if(incoming){viewState="incoming";cursorRequested(1)}else if(incomingTextPending){incomingTextPending=false;viewState="text";statusText=strings.t("status.textReceived");cursorRequested(0)}else{viewState=selectedDevice?"target":"nearby";cursorRequested(0)} focusRestoreRequested() }
  function acceptIncoming() { if (!incoming) return; send({command:"accept",request_id:incoming.requestId}); viewState="receiving"; transferPeer=incoming.sender; transferName=Model.incomingSummary(incoming.files, strings); progress=0 }
  function declineIncoming() {
    if (!incoming) return
    var requestId = incoming.requestId
    send({command:"decline",request_id:requestId})
    incomingQueue = Model.removeIncoming(incomingQueue, requestId)
    if (!incoming&&incomingTextPending) { incomingTextPending=false; viewState="text"; statusText=strings.t("status.textReceived") }
    else { viewState = incoming ? "incoming" : "nearby"; statusText = incoming ? strings.t("status.incomingTransfer") : strings.t("status.declined") }
    cursorRequested(incoming ? 1 : 0)
  }
  function finishIncoming(terminalState, message, completed) {
    activeIncomingSession = ""
    if (pendingOutgoing) return
    if (incoming) {
      viewState = "incoming"
      cursorRequested(1)
      statusText = strings.t("status.incomingTransfer")
      errorText = ""
      progress = 0
      return
    }
    if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText=strings.t("status.textReceived"); errorText=""; progress=0; return }
    viewState = terminalState
    transferWasOutgoing = false
    statusText = message
    errorText = terminalState === "error" ? message : ""
    if (completed) progress = 1
  }
  function finishOutgoing(terminalState, message, completed) {
    clearPendingOutgoing()
    if (incoming) {
      viewState = Model.viewAfterOutgoing(true, terminalState)
      cursorRequested(1)
      statusText = strings.t("status.incomingTransfer")
      errorText = ""
      progress = 0
      return
    }
    if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText=strings.t("status.textReceived"); errorText=""; progress=0; return }
    viewState = Model.viewAfterOutgoing(false, terminalState)
    transferWasOutgoing = true
    statusText = message
    errorText = terminalState === "error" ? message : ""
    if (completed) progress = 1
  }
  // Only a result the user is looking at is offered: a receive that finished
  // behind another transfer or request never reached the screen.
  function offerReceivedFile(session) {
    receivedFileType = ""
    receivedPreview = ""
    receivedFile = viewState === "success" && receivedSession === session && receivedFileCount === 1 ? lastReceivedPath : ""
    if (receivedFile === "") return
    cursorRequested(0)
    if (!fileTypeProbe.running) probeReceivedFile()
  }
  function probeReceivedFile() { fileTypeProbe.path = receivedFile; fileTypeProbe.running = true }
  // An answer about a file that is no longer the one on screen must not label
  // a different file. The file that replaced it could not be probed while this
  // probe ran, so it is probed now.
  function fileTypeProbed(code, type) {
    if (fileTypeProbe.path === receivedFile) { if (code === 0) { receivedFileType = type; previewReceivedFile() } }
    else if (receivedFile !== "") probeReceivedFile()
  }
  function previewReceivedFile() {
    var mode = Model.imagePreviewMode(receivedFileType)
    if (mode === "direct") receivedPreview = receivedFile
    else if (mode === "convert" && !previewMaker.running) { previewMaker.path = receivedFile; previewMaker.running = true }
  }
  // Like the probe, a conversion answers for the file it was started on. One
  // that finished for a file no longer shown starts the conversion of the file
  // that replaced it.
  function previewMade(code, path) {
    if (previewMaker.path === receivedFile) { if (code === 0 && path !== "") receivedPreview = path }
    else if (receivedFile !== "" && Model.imagePreviewMode(receivedFileType) === "convert") previewReceivedFile()
  }
  function finishText() { incomingText=""; incomingTextPending=false; viewState="nearby"; cursorRequested(0); startDiscovery() }
  function finishTerminal() { viewState="nearby"; cursorRequested(0); startDiscovery() }
  function handleBackendExit(code) {
    var restarting=restartPending; restartPending=false
    backendReady=false; discoveryActive=false; incomingQueue=[]; incomingTextPending=false; activeIncomingSession=""; incomingPinUpdating=false; pendingIncomingPinEnabled=null; incomingPinError=""; incomingPinCleared(); clearPendingOutgoing()
    if (shutdownPending) { finishReceiverShutdown(); return }
    if (!receiverEnabled) { statusText=strings.t("status.turnedOff"); errorText=""; return }
    if (backendVersionMismatch) return
    if (!backendAcceptedThisRun) {
      // An early exit alone does not identify its cause. The helper reports a
      // confirmed bind conflict as a structured event before it exits; all
      // other pre-ready failures remain generic. Both keep the existing
      // bounded retry schedule so a transient conflict can recover.
      if (backendStartupFailureCode === "receiver_security_settings_invalid") {
        reportFailure(strings.t("status.securityInvalid"),
          strings.t("status.securityInvalidDetail"))
      } else if (backendRestart.attempts < 4) {
        reportFailure(strings.t("status.retrying"), strings.t("status.retryingDetail"))
      } else if (backendStartupFailureCode === "receiver_port_in_use") {
        var port = backendStartupFailurePort > 0 ? backendStartupFailurePort : 53317
        reportFailure(strings.t("status.portInUse", [strings.number(port)]),
          strings.t("status.portInUseDetail", [strings.number(port)]))
      } else if (downloadDir !== "") {
        // The helper does not say why it stopped before it was ready, and a
        // chosen folder that has gone away is the one cause the user set up.
        reportFailure(strings.t("status.couldNotStart"),
          strings.t("status.couldNotStartFolderDetail", [downloadDir, strings.t("folder.useDownloads")]))
      } else {
        reportFailure(strings.t("status.couldNotStart"), strings.t("status.couldNotStartDetail"))
      }
    } else {
      reportFailure(code===0 ? strings.t("status.receiverStopped") : strings.t("status.receiverUnavailable"),
        code===0 ? strings.t("status.receiverStopped") : strings.t("status.receiverUnavailable"))
      if (restarting) reportFailure(strings.t("status.restarting"), strings.t("status.restartingDetail"))
      if (viewState.indexOf("incoming_pin_")===0) { viewState="error"; reportFailure(strings.t("status.backendStopped"), strings.t("status.backendStoppedPin")) }
      else if (viewState==="sending"||viewState==="receiving"||viewState==="pin"||viewState==="incoming") { viewState="error"; reportFailure(strings.t("status.backendStopped"), strings.t("status.backendStoppedTransfer")) }
    }
    if (backendStartupFailureCode !== "receiver_security_settings_invalid" && backendRestart.attempts < 4) { backendRestart.attempts++; backendRestart.interval=Math.min(30000,1000*Math.pow(2,backendRestart.attempts-1)); backendRestart.restart() }
  }
  function handleEvent(event) {
    if (!event || !event.event) return
    if (backendVersionMismatch && event.event !== "ready") return
    if (event.event === "startup_failed") {
      backendStartupFailureCode=String(event.code || "")
      backendStartupFailurePort=Number(event.port) || 0
    }
    else if (event.event === "ready") {
      helperVersion=String(event.helperVersion || "")
      helperMissing=false
      if (!Model.helperSatisfies(requiredHelperVersion, helperVersion)) {
        backendReady=false
        backendVersionMismatch=true
        reportFailure(strings.t("helper.outOfDate"), helperUpdateDetail)
        send({command:"shutdown"})
        return
      }
      saveDirectory=String(event.directory || ""); backendAcceptedThisRun=true; backendReady=true; backendVersionMismatch=false; backendRestart.attempts=0; statusText=strings.t("status.readyToReceive"); errorText=""; if(anyViewOpen&&viewState==="nearby")startDiscovery()
    }
    else if (event.event === "incoming_pin_state") {
      incomingPinEnabled=event.enabled===true
      if(pendingIncomingPinEnabled!==null){var enabled=pendingIncomingPinEnabled===true;var settingsStillVisible=viewState.indexOf("incoming_pin_")===0;pendingIncomingPinEnabled=null;incomingPinUpdating=false;incomingPinError="";incomingPinCleared();if(settingsStillVisible){viewState="incoming_pin_settings";statusText=enabled?strings.t("status.pinEnabled"):strings.t("status.pinDisabled");cursorRequested(0);focusRestoreRequested()}}
    }
    else if (event.event === "incoming_pin_update_failed") {
      pendingIncomingPinEnabled=null;incomingPinUpdating=false;incomingPinError=String(event.message||strings.t("pin.updateFailed"));incomingPinCleared();if(viewState==="incoming_pin_edit")incomingPinFocusRequested()
    }
    else if (event.event === "peer_snapshot") { devices=Model.snapshotDevices(event.devices); statusText=devices.length ? strings.t("status.ready") : strings.t("status.lookingNearby") }
    else if (event.event === "device") { devices=Model.upsertDevice(devices,event.device); statusText=devices.length ? strings.t("status.ready") : strings.t("status.lookingNearby") }
    else if (event.event === "discovery_started") discoveryActive=true
    else if (event.event === "discovery_stopped") discoveryActive=false
    else if (event.event === "incoming_request") {
      if(viewState.indexOf("incoming_pin_")===0){incomingPinError="";incomingPinCleared()}
      incomingQueue=Model.enqueueIncoming(incomingQueue,event); if(!incomingTextPending)incomingText=""
      var incomingTakesView = viewState!=="sending" && viewState!=="receiving" && viewState!=="pin"
      if (incomingTakesView) { viewState="incoming"; cursorRequested(1) }
      if (notificationNeeded(incomingTakesView && incoming && incoming.requestId===event.requestId))
        Quickshell.execDetached(Model.incomingNotificationCommand(event.requestId, ["--app-name","Nearby","--urgency","normal",strings.t("status.incomingTransfer"),strings.t("notify.incomingBody", [Model.notificationText(event.sender), Model.notificationText(Model.incomingSummary(event.files, strings))])]))
    }
    else if (event.event === "incoming_text") {
      if(viewState.indexOf("incoming_pin_")===0){incomingPinError="";incomingPinCleared()}
      incomingText=String(event.text || ""); transferPeer=String(event.sender || ""); incomingTextPending=pendingOutgoing!==null; if (!pendingOutgoing) { viewState="text"; stopDiscovery() }
      if (notificationNeeded(!incomingTextPending))
        Quickshell.execDetached(["omarchy-notification-send","--app-name","Nearby","--urgency","normal",strings.t("status.textReceived"),strings.t("notify.textBody", [Model.notificationText(event.sender)])])
    }
    else if (event.event === "incoming_accepted") { incomingQueue=Model.removeIncoming(incomingQueue,event.requestId) }
    else if (event.event === "incoming_expired") {
      var expiredWasCurrent=incoming&&incoming.requestId===event.requestId
      incomingQueue=Model.removeIncoming(incomingQueue,event.requestId)
      if (expiredWasCurrent&&(viewState==="incoming"||(viewState==="receiving"&&activeIncomingSession===""))) {
        if (incoming) { statusText=strings.t("status.incomingTransfer"); cursorRequested(1) }
        else if (incomingTextPending) { incomingTextPending=false; viewState="text"; statusText=strings.t("status.textReceived"); errorText="" }
        else { viewState="error"; errorText=strings.t("status.requestExpired"); statusText=errorText }
      }
    }
    else if (event.event === "incoming_progress") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; if(pendingOutgoing)return; viewState="receiving"; transferName=String(event.name); transferPeer=String(event.sender); progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "file_received") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; if(receivedSession!==activeIncomingSession){receivedSession=activeIncomingSession;receivedFileCount=0} receivedFileCount++; lastReceivedPath=String(event.path); if(pendingOutgoing)return; transferName=String(event.name); transferPeer=String(event.sender) }
    else if (event.event === "incoming_done") { if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("success",strings.t("status.received"),true); offerReceivedFile(String(event.sessionId)) }
    else if (event.event === "incoming_cancelled") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("error",strings.t("status.transferCancelled"),false) }
    else if (event.event === "incoming_failed") { if(activeIncomingSession===""){if(viewState!=="receiving")return;activeIncomingSession=String(event.sessionId)} if(activeIncomingSession!==String(event.sessionId))return; finishIncoming("error",String(event.message||strings.t("status.transferFailed")),false) }
    else if (event.event === "incoming_declined") { incomingQueue=Model.removeIncoming(incomingQueue,event.requestId) }
    else if (event.event === "outgoing_preparing") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; transferName=String(event.name); transferPeer=String(event.target); progress=0; stopDiscovery() }
    else if (event.event === "outgoing_progress") { if(String(event.transferId)!==outgoingTransferId)return; viewState="sending"; if(event.name) transferName=String(event.name); progress=event.total>0 ? event.bytes/event.total : 0 }
    else if (event.event === "outgoing_done") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("success", strings.t("status.sent"), true) }
    else if (event.event === "outgoing_cancelled") { if(String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", strings.t("status.transferCancelled"), false) }
    else if (event.event === "outgoing_pin_required") { if(String(event.transferId)!==outgoingTransferId)return; showPinPrompt("") }
    else if (event.event === "outgoing_invalid_pin") { if(String(event.transferId)!==outgoingTransferId)return; showPinPrompt(strings.t("pin.incorrect")) }
    else if (event.event === "outgoing_failed") { if(event.transferId && String(event.transferId)!==outgoingTransferId)return; finishOutgoing("error", String(event.message||strings.t("status.transferFailed")), false) }
    else if (event.event === "error") { viewState="error"; errorText=String(event.message||strings.t("status.transferFailed")); statusText=errorText }
  }

  Process {
    id: backend
    property bool launched: false
    command: [root.pluginDir + "/bin/nearby-helper-launcher"]
    // Not eligible to start until shell.json has been read. Defaulting to on
    // before then would let the helper bind the LocalSend port and announce
    // itself on the network for a user who had turned the receiver off.
    running: root.receiverConfigured && root.receiverEnabled && root.pluginVersion !== ""
    environment: root.downloadDir !== "" ? ({ XDG_DOWNLOAD_DIR: root.downloadDir }) : ({})
    stdinEnabled: true
    stdout: SplitParser { onRead: function(line) { root.handleEvent(Model.parseLine(line)) } }
    stderr: SplitParser { onRead: function(line) { console.warn("nearby backend", line) } }
    onStarted: {
      backend.launched=true
      root.backendAcceptedThisRun=false
      root.backendVersionMismatch=false
      root.helperMissing=false
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
      root.helperMissing=true
      root.reportFailure(strings.t("helper.unavailable"),
        strings.t("helper.unavailableDetail", [strings.t("helper.retry"), "./build.sh"]))
    }
    onExited: function(code) { root.handleBackendExit(code) }
  }
  Process {
    id: helperUpdater
    property bool launched: false
    command: [root.pluginDir + "/bin/nearby-repair-helper"]
    running: false
    stdout: SplitParser { onRead: function(line) { root.handleUpdaterEvent(Model.parseLine(line)) } }
    stderr: SplitParser { onRead: function(line) { console.warn("nearby helper repair", line) } }
    onStarted: helperUpdater.launched=true
    // Same missing-command signal the helper and the file chooser use: no exit
    // code ever arrives, so an absent repair command has to be caught here.
    onRunningChanged: {
      if (running || helperUpdater.launched) return
      root.helperUpdating=false
      root.helperUpdateStatus=""
      root.helperUpdateError=strings.t("helper.repairMissing", ["omarchy plugin update oma.nearby"])
    }
    onExited: function(code) { root.finishHelperUpdate(code) }
  }
  // Whether a received file can be previewed is decided from its bytes: the
  // name and the declared type are both the sender's to choose. A probe that
  // cannot run leaves the type empty, which only withholds the preview.
  Process {
    id: fileTypeProbe
    property string path: ""
    command: ["file", "--brief", "--mime-type", "--", fileTypeProbe.path]
    running: false
    stdout: StdioCollector { id: fileTypeOutput; waitForEnd: true }
    onExited: function(code) { root.fileTypeProbed(code, String(fileTypeOutput.text || "").trim()) }
  }
  // A converter that cannot run leaves the preview empty, as a failed probe does.
  Process {
    id: previewMaker
    property string path: ""
    command: [root.pluginDir + "/bin/nearby-preview-image", previewMaker.path]
    running: false
    stdout: StdioCollector { id: previewOutput; waitForEnd: true }
    onExited: function(code) { root.previewMade(code, String(previewOutput.text || "").split("\n")[0]) }
  }
  Timer { id: backendRestart; property int attempts: 0; interval: 1000; repeat: false; onTriggered: if (root.receiverEnabled && !backend.running) root.bindBackendRunning() }
  Timer { id: receiverShutdownFallback; interval: 250; repeat: false; onTriggered: root.finishReceiverShutdown() }
}
