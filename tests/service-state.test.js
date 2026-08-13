const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const Model = require("../Model.js")

const source = fs.readFileSync(require.resolve("../Service.qml"), "utf8")
const manifest = JSON.parse(fs.readFileSync(require.resolve("../manifest.json"), "utf8"))

// The bar builds one widget per monitor, so anything that may exist only once
// has to live behind the `service` kind, which the shell loads a single time.
// A helper owned by the widget was started once per screen and every instance
// after the first lost the race for the LocalSend port.
assert.ok(manifest.kinds.includes("service"),
  "the helper is a singleton, so the plugin must declare a service kind")
assert.equal(manifest.entryPoints.service, "Service.qml",
  "the service kind needs an entry point for the shell to load it")
assert.match(source, /command:\s*\[root\.pluginDir \+ "\/bin\/omarchy-nearby-helper"\]/,
  "the helper must be spawned from the service, not from a per-monitor widget")
assert.match(source, /IpcHandler\s*\{\s*target:\s*"oma\.nearby"/,
  "the IPC target must be registered once, from the service")

// Opening a popup is per monitor even though the state behind it is not, so
// those calls are handed back to the shell to route to the focused screen.
for (const method of ["summon", "hide", "toggle"]) {
  assert.match(source, new RegExp(`summonView\\("${method}"\\)`),
    `${method} must route through the shell rather than a fixed widget instance`)
}

// A helper that is missing never reaches onExited: Quickshell drops `running`
// without an exit code, the same way the file chooser reports a missing
// command. That is the failure reinstalling actually fixes.
assert.match(source, /onRunningChanged:\s*\{[\s\S]*?if \(backend\.launched\) return/,
  "a helper that never launched must be reported from onRunningChanged")
assert.match(source, /Run the Nearby installer again or build it with \.\/build\.sh\./,
  "the reinstall hint belongs to the missing-helper case")

function extractFunction(name) {
  const marker = `function ${name}`
  const start = source.indexOf(marker)
  assert.notEqual(start, -1, `Service.qml must define ${name}`)
  const brace = source.indexOf("{", start)
  let depth = 0
  let quote = ""
  let escaped = false
  for (let index = brace; index < source.length; index++) {
    const character = source[index]
    if (quote) {
      if (escaped) escaped = false
      else if (character === "\\") escaped = true
      else if (character === quote) quote = ""
      continue
    }
    if (character === '"' || character === "'") quote = character
    else if (character === "{") depth++
    else if (character === "}" && --depth === 0) return source.slice(start, index + 1)
  }
  throw new Error(`Unterminated ${name}`)
}

const functionNames = [
  "configure", "viewOpened", "viewClosed",
  "send", "persistReceiverEnabled", "toggleReceiver", "finishReceiverShutdown",
  "startDiscovery", "forceFullDiscovery", "stopDiscovery", "chooseDevice", "clearTarget",
  "failWith", "cancelOutgoing", "noteTextCopied",
  "clearPendingOutgoing", "dispatchPendingOutgoing", "beginOutgoing", "retryWithPin",
  "showPinPrompt", "cancelPin", "acceptIncoming", "declineIncoming",
  "finishIncoming", "finishOutgoing", "finishText", "finishTerminal",
  "handleBackendExit", "handleEvent",
]

function engine(initial = {}) {
  const sent = []
  const cursors = []
  const signals = {pinCleared: 0, pinFocusRequested: 0, focusRestoreRequested: 0}
  const context = {
    Model,
    Date: {now: () => 1000},
    Quickshell: {execDetached: () => {}},
    backend: {running: true, write: line => sent.push(JSON.parse(line))},
    backendRestart: {attempts: 0, interval: 0, restart: () => {}, stop: () => {}},
    receiverShutdownFallback: {stop: () => {}, restart: () => {}},
    // Views own their cursor and PIN field, so the engine only signals at them.
    cursorRequested: index => cursors.push(index),
    pinCleared: () => { signals.pinCleared++ },
    pinFocusRequested: () => { signals.pinFocusRequested++ },
    focusRestoreRequested: () => { signals.focusRestoreRequested++ },
    shell: null,
    moduleEntryId: "oma.nearby",
    pluginSettings: {},
    receiverConfigured: true,
    openViewCount: 1,
    anyViewOpen: true,
    shutdownPending: false,
    pluginVersion: "1.0.4",
    backendVersionMismatch: false,
    backendReady: true,
    backendAcceptedThisRun: true,
    receiverEnabled: true,
    discoveryActive: false,
    devices: [],
    selectedDevice: {fingerprint: "phone", alias: "Phone"},
    viewState: "target",
    statusText: "",
    errorText: "",
    incomingQueue: [],
    incomingText: "",
    incomingTextPending: false,
    lastReceivedPath: "",
    progress: 0,
    transferName: "",
    transferPeer: "",
    activeIncomingSession: "",
    outgoingTransferId: "",
    pendingOutgoing: null,
    pinError: "",
    transferSequence: 0,
    ...initial,
  }
  Object.defineProperty(context, "incoming", {get() { return Model.currentIncoming(context.incomingQueue) }})
  vm.createContext(context)
  vm.runInContext(functionNames.map(extractFunction).join("\n"), context)
  context.sent = sent
  context.cursors = cursors
  context.signals = signals
  return context
}

function incoming(requestId, sender = requestId) {
  return {event: "incoming_request", requestId, sender, files: [{name: `${requestId}.txt`}], total: 1}
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.handleEvent(incoming("b"))
  state.declineIncoming()
  assert.deepEqual(state.incomingQueue.map(item => item.requestId), ["b"])
  assert.equal(state.incoming.requestId, "b")
  assert.equal(state.viewState, "incoming")
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.handleEvent(incoming("b"))
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.equal(state.incoming.requestId, "b")
  assert.equal(state.viewState, "incoming")
  state.handleEvent({event: "incoming_expired", requestId: "unknown"})
  assert.equal(state.incoming.requestId, "b")
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.acceptIncoming()
  assert.equal(state.viewState, "receiving")
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.notEqual(state.viewState, "receiving",
    "an expired approval must not leave the UI waiting for a receive session that cannot start")
  assert.equal(state.activeIncomingSession, "")
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "one"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_preparing", transferId, name: "message.txt", target: "Phone"})
  state.handleEvent({event: "outgoing_done", transferId})
  assert.equal(state.incoming.requestId, "a")
  assert.equal(state.viewState, "incoming")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "original"})
  const firstId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId: firstId})
  state.handleEvent(incoming("a"))
  state.retryWithPin("000000")
  const retryId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_invalid_pin", transferId: retryId})
  state.handleEvent(incoming("b"))
  state.retryWithPin("123456")
  assert.equal(state.sent.at(-1).text, "original")
  assert.equal(state.sent.at(-1).pin, "123456")
  assert.deepEqual(state.incomingQueue.map(item => item.requestId), ["a", "b"])
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.equal(state.viewState, "pin")
  assert.equal(state.pendingOutgoing.text, "original")
  assert.equal(state.incoming.requestId, "b")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "only once"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.retryWithPin("123456")
  state.retryWithPin("123456")
  assert.equal(state.sent.filter(command => command.pin === "123456").length, 1,
    "a repeated submit while a PIN retry is active must not dispatch a duplicate")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "no pin"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  const before = state.sent.length
  state.retryWithPin("")
  assert.equal(state.sent.length, before, "an empty PIN must not dispatch a transfer")
  assert.equal(state.pinError, "Enter the receiver PIN")
  assert.ok(state.signals.pinFocusRequested > 0,
    "an empty PIN must ask the open view to focus its own input")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "cancel retry"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.retryWithPin("123456")
  const retryId = state.outgoingTransferId
  state.cancelPin()
  assert.deepEqual(state.sent.filter(command => command.command === "cancel_outgoing"),
    [{command: "cancel_outgoing", transfer_id: retryId}],
    "leaving PIN while its retry is in flight must cancel that helper transfer exactly once")
  assert.equal(state.pendingOutgoing, null)
  assert.equal(state.outgoingTransferId, "")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "keep me"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  const pending = state.pendingOutgoing
  state.viewClosed()
  assert.equal(state.discoveryActive, false, "the last view closing must stop discovery")
  assert.equal(state.pendingOutgoing, pending, "closing a popup must leave a PIN retry pending")
  assert.equal(state.viewState, "pin")
}

{
  // Discovery follows the popup, and there is one popup per monitor, so it may
  // only stop once the last of them has closed.
  const state = engine({openViewCount: 0, viewState: "nearby", discoveryActive: false})
  state.viewOpened()
  state.viewOpened()
  assert.equal(state.discoveryActive, true)
  state.viewClosed()
  assert.equal(state.discoveryActive, true,
    "one monitor closing must not stop discovery while another popup is open")
  state.viewClosed()
  assert.equal(state.discoveryActive, false)
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "pending"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId})
  state.handleEvent({event: "incoming_cancelled", sessionId: "unknown"})
  state.handleEvent({event: "incoming_failed", sessionId: "unknown", message: "late"})
  assert.equal(state.viewState, "pin")
  assert.equal(state.pendingOutgoing.text, "pending")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "outgoing"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_preparing", transferId, name: "message.txt", target: "Phone"})
  state.handleEvent({event: "incoming_cancelled", sessionId: "unknown"})
  assert.equal(state.viewState, "sending")
  assert.equal(state.outgoingTransferId, transferId)
  state.handleEvent({event: "outgoing_done", transferId: "old"})
  assert.equal(state.outgoingTransferId, transferId)
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "outgoing"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId})
  state.handleEvent({event: "incoming_text", sessionId: "incoming-text", sender: "Alice", text: "hello"})
  assert.equal(state.viewState, "pin", "incoming text must not hide an unrelated PIN prompt")
  assert.ok(state.pendingOutgoing)
  state.handleEvent({event: "outgoing_done", transferId: "old"})
  assert.equal(state.viewState, "pin")
  state.retryWithPin("123456")
  const retryId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_done", transferId: retryId})
  assert.equal(state.viewState, "text", "deferred incoming text must become accessible after outgoing completion")
  assert.equal(state.incomingText, "hello")
}

{
  const state = engine({viewState: "receiving"})
  state.handleEvent({event: "incoming_cancelled", sessionId: "session-a"})
  assert.equal(state.viewState, "error")
  assert.equal(state.activeIncomingSession, "")
  state.handleEvent({event: "incoming_done", sessionId: "session-a"})
  assert.equal(state.viewState, "error", "late terminal events must not resurrect a completed session")
}

{
  const state = engine({viewState: "receiving"})
  state.handleEvent({event: "incoming_progress", sessionId: "a", name: "a.txt", sender: "Alice", bytes: 1, total: 2})
  state.handleEvent({event: "incoming_cancelled", sessionId: "a"})
  state.handleEvent({event: "incoming_progress", sessionId: "a", name: "a.txt", sender: "Alice", bytes: 2, total: 2})
  assert.equal(state.activeIncomingSession, "", "late progress must not resurrect cancelled incoming A")
  assert.equal(state.viewState, "error")
}

{
  const state = engine({viewState: "receiving"})
  state.handleEvent({event: "incoming_progress", sessionId: "b", name: "b.txt", sender: "Bob", bytes: 1, total: 2})
  state.handleEvent({event: "incoming_progress", sessionId: "a", name: "a.txt", sender: "Alice", bytes: 2, total: 2})
  assert.equal(state.activeIncomingSession, "b")
  assert.equal(state.transferName, "b.txt")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "outgoing"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "incoming_text", sender: "Alice", text: "deferred"})
  state.handleEvent(incoming("files"))
  state.handleEvent({event: "outgoing_done", transferId})
  assert.equal(state.viewState, "incoming")
  state.declineIncoming()
  assert.equal(state.viewState, "text", "deferred text must follow the last queued file request")
  assert.equal(state.incomingText, "deferred")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "first"})
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "duplicate"})
  assert.equal(state.sent.filter(command => command.command === "send_text").length, 1,
    "a second begin before helper confirmation must not replace the active outgoing")
  assert.equal(state.pendingOutgoing.text, "first")
}

{
  const state = engine({backend: {running: false, write: () => { throw new Error("must not write") }}})
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "offline"})
  assert.equal(state.pendingOutgoing, null, "backend downtime must not create a pending outgoing phantom")
  assert.equal(state.outgoingTransferId, "")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "first"})
  const first = state.sent.at(-1)
  state.handleEvent({event: "outgoing_done", transferId: first.transfer_id})
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "second"})
  const second = state.sent.at(-1)
  assert.notEqual(first.transfer_id, second.transfer_id)
  assert.equal(first.text, "first")
  assert.equal(second.text, "second")
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.handleEvent(incoming("b"))
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "pending"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.activeIncomingSession = "session-a"
  state.finishReceiverShutdown()
  assert.equal(state.incomingQueue.length, 0)
  assert.equal(state.incoming, null)
  assert.equal(state.activeIncomingSession, "")
  assert.equal(state.pendingOutgoing, null)
  assert.equal(state.outgoingTransferId, "")
  assert.equal(state.viewState, "nearby")
}

{
  const state = engine()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "pending"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.handleBackendExit(1)
  assert.equal(state.pendingOutgoing, null)
  assert.equal(state.outgoingTransferId, "")
  assert.equal(state.viewState, "error")
  assert.match(state.errorText, /stopped|unavailable/i)
}

{
  const state = engine()
  state.handleEvent(incoming("a"))
  state.handleBackendExit(1)
  assert.equal(state.incoming, null)
  assert.equal(state.activeIncomingSession, "")
  assert.equal(state.viewState, "error", "helper exit must not leave an empty incoming view")
}

{
  // A helper that exits before announcing itself is the port-conflict case.
  // It used to report a permanent failure and skip the retry schedule, so a
  // port that freed up was never picked back up.
  const restarts = []
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 0, interval: 0, restart() { restarts.push(this.attempts) }, stop: () => {}},
  })
  state.handleBackendExit(1)
  assert.equal(state.backendRestart.attempts, 1,
    "a helper that never became ready must still be retried")
  assert.deepEqual(restarts, [1])
  assert.doesNotMatch(state.errorText, /installer|build\.sh/,
    "a helper that ran and exited is not fixed by reinstalling the plugin")
}

{
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 4, interval: 0, restart: () => { throw new Error("must not retry") }, stop: () => {}},
  })
  state.handleBackendExit(1)
  assert.match(state.errorText, /53317/,
    "a port conflict that outlasts the retry schedule must name the port it lost")
}

{
  const state = engine()
  state.handleEvent({event: "incoming_text", sender: "Alice", text: "clipboard"})
  assert.equal(state.viewState, "text")
  state.finishText()
  assert.equal(state.viewState, "nearby", "received text must have an exit independent of wl-copy completion")
  assert.equal(state.incomingText, "")
}

{
  const state = engine({viewState: "text", incomingText: "copied"})
  state.noteTextCopied()
  assert.equal(state.viewState, "success")
  assert.equal(state.statusText, "Received")
  const late = engine({viewState: "nearby"})
  late.noteTextCopied()
  assert.equal(late.viewState, "nearby",
    "late wl-copy completion must not reopen a text result after the user exits")
}

{
  const state = engine({viewState: "success", discoveryActive: false})
  state.finishTerminal()
  assert.equal(state.viewState, "nearby")
  assert.equal(state.discoveryActive, true, "Done must restart discovery")
  assert.equal(state.sent.at(-1).command, "discovery_start")
}

{
  const state = engine({receiverConfigured: false, receiverEnabled: true})
  state.configure("oma.nearby", {receiverEnabled: false})
  assert.equal(state.receiverEnabled, false, "a persisted off switch must survive a restart")
  state.configure("oma.nearby", {receiverEnabled: false})
  assert.equal(state.moduleEntryId, "oma.nearby")
}

{
  // Every monitor's widget pushes the same settings, so configure has to be
  // idempotent rather than clobbering a live receiver state.
  const state = engine({receiverConfigured: false, receiverEnabled: true})
  state.configure("oma.nearby", {receiverEnabled: true})
  state.receiverEnabled = false
  state.configure("oma.nearby", {receiverEnabled: true})
  assert.equal(state.receiverEnabled, false,
    "a second view configuring the engine must not revive a receiver the user turned off")
}

assert.match(source,
  /function failWith\(message\)\s*\{\s*viewState="error";\s*errorText=message;\s*statusText=errorText\s*\}/,
  "failWith must keep statusText aligned with errorText for every failure that reaches it")

console.log("service state tests passed")
