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
assert.match(source, /command:\s*\[root\.pluginDir \+ "\/bin\/nearby-helper-launcher"\]/,
  "the helper launcher must be spawned from the service, not from a per-monitor widget")
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
assert.match(source, /run the installer again, or build it with \.\/build\.sh\./,
  "the reinstall hint belongs to the missing-helper case")
assert.match(source, /onStarted:\s*\{[\s\S]*?backendStartupFailureCode=""[\s\S]*?backendStartupFailurePort=0/,
  "each helper attempt must discard a stale startup cause before it runs")
assert.match(source, /id: backendRestart[\s\S]*?onTriggered:[^\n]*bindBackendRunning\(\)/,
  "the retry timer must restore the Process.running binding")
assert.doesNotMatch(source, /backend\.running\s*=\s*true/,
  "a plain retry assignment would permanently remove the Process.running binding")

// The tracked updater asks the launcher to prefetch the immutable helper into
// XDG data. install.sh also owns the checkout, so the panel must not call it.
assert.match(source, /command:\s*\[root\.pluginDir \+ "\/bin\/nearby-update-helper"\]/,
  "the in-panel update must run the helper prefetch updater")
assert.doesNotMatch(source, /install\.sh"\]/,
  "install.sh owns the git checkout and declines a dirty one; the panel must not call it")
assert.match(source, /id: helperUpdater[\s\S]*?onRunningChanged:\s*\{\s*\n\s*if \(running \|\| helperUpdater\.launched\) return/,
  "an updater that never launched must be reported the same way a missing helper is")
assert.match(source, /function finishHelperUpdate[\s\S]*?if \(!backend\.running\) bindBackendRunning\(\)/,
  "a successful update must restore the Process.running binding the mismatch shutdown wrote over")
assert.match(source, /Model\.helperSatisfies\(requiredHelperVersion, helperVersion\)/,
  "the helper is checked against the floor the manifest declares, not against an exact version")

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

// The eligibility rule is a binding, not a function, so it is pulled out of
// the source and evaluated directly. That is what covers the startup window:
// checking state after the fact would pass even if the helper had already
// bound the port.
function backendEligible(state) {
  const match = source.match(/id: backend[\s\S]*?\n\s*running:\s*([^\n]+)/)
  assert.notEqual(match, null, "Service.qml must bind the helper's running state")
  const expression = match[1].trim()
  assert.match(expression, /receiverConfigured/,
    "eligibility must depend on having read the config, not only on the receiver switch")
  return vm.runInNewContext(`(${expression})`, {root: state})
}

// configEntry is a binding too, and with the scoped shell it derives from
// effectiveShellConfig instead of shell.shellConfig. Evaluating it is what
// covers the modern host: the fallback file becomes the receiver's source.
function configEntry(state) {
  return vm.runInNewContext(`(${extractBinding("configEntry")})`, {
    Model,
    manifestPluginId: "oma.nearby",
    shell: state.shell,
    get effectiveShellConfig() { return state.effectiveShellConfig },
  })
}

// The version floor is a binding too, so it comes out of the source rather
// than being restated. A manifest with no floor falls back to the version it
// ships with, and a test that hardcoded that fallback would not notice it
// changing under the check that depends on it.
function requiredHelperVersion(state) {
  const match = source.match(/readonly property string requiredHelperVersion:\s*([^\n]+)/)
  assert.notEqual(match, null, "Service.qml must derive the helper version floor")
  return vm.runInNewContext(`(${match[1].trim()})`, {
    minHelperVersion: state.minHelperVersion,
    pluginVersion: state.pluginVersion,
  })
}

// What the panel offers is a chain of derived bindings, and each one is pulled
// out of the source rather than restated here: they decide whether the update
// row appears and what it says, so a copy could agree with the test while
// disagreeing with the plugin.
function extractBinding(name) {
  const match = source.match(new RegExp(
    `readonly property \\w+ ${name}:\\s*([\\s\\S]*?)(?=\\n\\s*(?://|readonly property|property |function |[A-Z]\\w*\\s*\\{))`))
  assert.notEqual(match, null, `Service.qml must bind ${name}`)
  return match[1].trim()
}

function helperDerived(state) {
  const scope = {
    Model,
    helperVersion: state.helperVersion,
    pluginVersion: state.pluginVersion,
    minHelperVersion: state.minHelperVersion,
    helperMissing: state.helperMissing,
    backendVersionMismatch: state.backendVersionMismatch,
    requiredHelperVersion: requiredHelperVersion(state),
  }
  for (const name of ["helperUpdateOffered", "helperUpdateDetail"]) {
    scope[name] = vm.runInNewContext(`(${extractBinding(name)})`, scope)
  }
  return scope
}

const functionNames = [
  "viewOpened", "viewClosed", "notificationNeeded",
  "send", "utf8ByteLength", "bindBackendRunning", "persistReceiverEnabled", "toggleReceiver", "finishReceiverShutdown",
  "startDiscovery", "forceFullDiscovery", "stopDiscovery", "chooseDevice", "clearTarget",
  "openIncomingPinSettings", "beginIncomingPinEdit", "requestDisableIncomingPin",
  "cancelIncomingPinSettings", "submitIncomingPin", "confirmDisableIncomingPin",
  "failWith", "cancelOutgoing", "noteTextCopied",
  "clearPendingOutgoing", "dispatchPendingOutgoing", "beginOutgoing", "retryWithPin",
  "showPinPrompt", "cancelPin", "acceptIncoming", "declineIncoming",
  "finishIncoming", "finishOutgoing", "finishText", "finishTerminal",
  "handleBackendExit", "handleEvent",
  "reportFailure", "startHelperUpdate", "handleUpdaterEvent", "finishHelperUpdate",
]

function engine(initial = {}) {
  const sent = []
  const cursors = []
  const notified = []
  const signals = {pinCleared: 0, pinFocusRequested: 0, incomingPinCleared: 0, incomingPinFocusRequested: 0, focusRestoreRequested: 0}
  const context = {
    Model,
    Date: {now: () => 1000},
    Quickshell: {execDetached: command => notified.push(command)},
    Qt: {binding: callback => ({callback})},
    backend: {running: true, write: line => sent.push(JSON.parse(line))},
    backendRestart: {attempts: 0, interval: 0, restart: () => {}, stop: () => {}},
    receiverShutdownFallback: {stop: () => {}, restart: () => {}},
    // Views own their cursor and PIN field, so the engine only signals at them.
    cursorRequested: index => cursors.push(index),
    pinCleared: () => { signals.pinCleared++ },
    pinFocusRequested: () => { signals.pinFocusRequested++ },
    incomingPinCleared: () => { signals.incomingPinCleared++ },
    incomingPinFocusRequested: () => { signals.incomingPinFocusRequested++ },
    focusRestoreRequested: () => { signals.focusRestoreRequested++ },
    shell: null,
    fileShellConfig: null,
    moduleEntryId: "oma.nearby",
    pluginSettings: {},
    receiverConfigured: true,
    openViewCount: 1,
    anyViewOpen: true,
    shutdownPending: false,
    pluginVersion: "1.0.4",
    minHelperVersion: "",
    helperVersion: "",
    helperMissing: false,
    helperUpdating: false,
    helperUpdateInstalled: false,
    helperUpdateStatus: "",
    helperUpdateError: "",
    helperUpdater: {running: false, launched: false},
    backendVersionMismatch: false,
    backendStartupFailureCode: "",
    backendStartupFailurePort: 0,
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
    incomingPinEnabled: false,
    incomingPinUpdating: false,
    pendingIncomingPinEnabled: null,
    incomingPinError: "",
    transferSequence: 0,
    maxOutgoingTextBytes: 1024 * 1024,
    ...initial,
  }
  Object.defineProperty(context, "incoming", {get() { return Model.currentIncoming(context.incomingQueue) }})
  Object.defineProperty(context, "requiredHelperVersion", {
    enumerable: true,
    get() { return requiredHelperVersion(context) },
  })
  Object.defineProperty(context, "hasLegacyShellConfig", {
    enumerable: true,
    get() { return !!(context.shell && context.shell.shellConfig) },
  })
  Object.defineProperty(context, "effectiveShellConfig", {
    enumerable: true,
    get() { return context.hasLegacyShellConfig ? context.shell.shellConfig : context.fileShellConfig },
  })
  for (const derived of ["helperUpdateOffered", "helperUpdateDetail"]) {
    Object.defineProperty(context, derived, {
      enumerable: true,
      get() { return helperDerived(context)[derived] },
    })
  }
  vm.createContext(context)
  vm.runInContext(functionNames.map(extractFunction).join("\n"), context)
  context.sent = sent
  context.cursors = cursors
  context.notified = notified
  context.signals = signals
  return context
}

function notificationTitles(state) {
  return state.notified
    .filter(command => command[0] === "omarchy-notification-send")
    .map(command => command[5])
}

function notificationCommands(state) {
  return state.notified.map(command => Array.from(command))
}

{
  const exact = engine()
  exact.beginOutgoing({kind: "text", device: exact.selectedDevice, text: "x".repeat(1024 * 1024)})
  assert.equal(exact.sent.length, 1, "text at the byte limit must be sent")

  const oversized = engine()
  oversized.beginOutgoing({kind: "text", device: oversized.selectedDevice, text: "x".repeat(1024 * 1024 + 1)})
  assert.equal(oversized.sent.length, 0, "text above the byte limit must not reach the helper")
  assert.equal(oversized.pendingOutgoing, null)
  assert.match(oversized.errorText, /maximum 1 MiB/)

  assert.equal(exact.utf8ByteLength("ñ".repeat(512 * 1024)), 1024 * 1024,
    "the UI limit must count UTF-8 bytes rather than JavaScript code units")
  assert.equal(exact.utf8ByteLength("😀"), 4,
    "a surrogate pair must count as one four-byte UTF-8 scalar")
}

{
  // A retry must reinstall the declarative Process.running binding. Assigning
  // a plain true removes it, so a later OFF -> ON toggle cannot start the
  // helper after a port conflict has exhausted the retry budget.
  const state = engine({backend: {running: false, write: () => {}}})
  state.root = state
  state.bindBackendRunning()
  assert.equal(typeof state.backend.running.callback, "function")
  assert.equal(state.backend.running.callback(), true)
  state.receiverEnabled = false
  assert.equal(state.backend.running.callback(), false)
  state.receiverEnabled = true
  assert.equal(state.backend.running.callback(), true,
    "the restored binding must make OFF -> ON eligible to start again")
}

// A source-only update moves the plugin ahead of a helper that can still be
// driven. That used to stop the plugin dead on every release; the floor is
// what keeps a compatible helper working.
{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.0.6", backendReady: false, viewState: "nearby",
  })
  state.handleEvent({event: "ready", helperVersion: "1.0.7"})
  assert.equal(state.backendVersionMismatch, false)
  assert.equal(state.backendReady, true, "a helper above the floor must keep working after a source update")
  assert.equal(state.sent.some(command => command.command === "shutdown"), false)
}

// Below the floor is still a stop, and the message has to name both versions:
// "run the installer again" was the old text and it did not say what was wrong.
//
// The hero renders statusText uppercased and letterspaced, so it gets a label
// and the body gets the sentence. Assigning the sentence to both is what made
// the popup print the same text twice, once cropped mid-word.
{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.1.0", backendReady: false, viewState: "nearby",
  })
  state.handleEvent({event: "ready", helperVersion: "1.0.7"})
  assert.equal(state.backendVersionMismatch, true)
  assert.equal(state.backendReady, false)
  assert.deepEqual(state.sent.at(-1), {command: "shutdown"})
  assert.match(state.errorText, /1\.1\.0/)
  assert.match(state.errorText, /1\.0\.7/)
  assert.equal(state.statusText, "Helper out of date")
  assert.notEqual(state.statusText, state.errorText,
    "the hero label and the body detail must not be the same string")
  assert.ok(state.statusText.length <= 24,
    `the hero label must fit uppercased and letterspaced, got ${state.statusText.length} characters`)
}

// Whatever a failure path sets, the label has to survive the hero's transform.
for (const [name, setup] of [
  ["port conflict", {backendStartupFailureCode: "receiver_port_in_use", backendStartupFailurePort: 53317}],
  ["invalid settings", {backendStartupFailureCode: "receiver_security_settings_invalid"}],
  ["generic startup failure", {backendStartupFailureCode: ""}],
]) {
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 4, interval: 0, restart: () => {}, stop: () => {}},
    backend: {running: false, write: () => {}},
    ...setup,
  })
  state.root = state
  state.handleBackendExit(1)
  assert.ok(state.statusText.length <= 26,
    `${name} label must fit the hero, got ${state.statusText.length} characters: ${state.statusText}`)
}

// Same rule for the other failures that reach the hero. The port conflict was
// the worst of them: 150 characters of advice cropped down to a fragment.
{
  const state = engine({
    backendAcceptedThisRun: false, backendRestart: {attempts: 4, interval: 0, restart: () => {}, stop: () => {}},
    backendStartupFailureCode: "receiver_port_in_use", backendStartupFailurePort: 53317,
    backend: {running: false, write: () => {}},
  })
  state.root = state
  state.handleBackendExit(1)
  assert.equal(state.statusText, "Port 53317 in use")
  assert.match(state.errorText, /Another LocalSend receiver/)
  assert.notEqual(state.statusText, state.errorText)
}

{
  const state = engine({
    backendAcceptedThisRun: false, backendRestart: {attempts: 4, interval: 0, restart: () => {}, stop: () => {}},
    backendStartupFailureCode: "receiver_security_settings_invalid",
    backend: {running: false, write: () => {}},
  })
  state.root = state
  state.handleBackendExit(1)
  assert.equal(state.statusText, "Security settings invalid")
  assert.match(state.errorText, /settings\.json/)
}

// The detail line is the only place the versions are stated, so the panel can
// drop the copies it used to print above and below it.
{
  const state = engine({pluginVersion: "1.1.0", minHelperVersion: "1.1.0", helperVersion: "1.0.7", backendVersionMismatch: true})
  assert.equal(state.helperUpdateDetail, "Needs helper 1.1.0 · installed 1.0.7")
}
{
  const state = engine({pluginVersion: "1.1.0", helperMissing: true})
  assert.equal(state.helperUpdateDetail, "The helper binary is missing.")
}
{
  const state = engine({pluginVersion: "1.1.0", minHelperVersion: "1.0.0", helperVersion: "1.0.7"})
  assert.equal(state.helperUpdateOffered, false,
    "independent plugin and helper versions must not create a false update")
  assert.equal(state.helperUpdateDetail, "")
}
{
  const state = engine({pluginVersion: "1.1.1-dev", minHelperVersion: "1.1.0", helperVersion: "1.1.0"})
  assert.equal(state.helperUpdateOffered, false,
    "a compatible helper must not offer an impossible optional update for a development checkout")
}
{
  const state = engine({
    pluginVersion: "1.1.1-dev", minHelperVersion: "1.1.1-dev", helperVersion: "1.1.0",
    backendVersionMismatch: true,
  })
  assert.equal(state.helperUpdateOffered, true,
    "an incompatible helper must still expose the recovery UI on a development checkout")
  assert.match(state.helperUpdateDetail, /Needs helper 1\.1\.1-dev/)
}

// No floor declared reads as the shipped version, which is the rule the plugin
// followed before the field existed.
{
  const state = engine({pluginVersion: "1.1.0", minHelperVersion: "", backendReady: false})
  assert.equal(state.requiredHelperVersion, "1.1.0")
  state.handleEvent({event: "ready", helperVersion: "1.0.7"})
  assert.equal(state.backendVersionMismatch, true)
}

// A helper ahead of the plugin is not a reason to refuse to start.
{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.1.0", backendReady: false, viewState: "nearby",
  })
  state.handleEvent({event: "ready", helperVersion: "1.2.0"})
  assert.equal(state.backendVersionMismatch, false)
  assert.equal(state.backendReady, true)
}

// A helper that reports nothing readable is not one to trust with a transfer.
{
  const state = engine({pluginVersion: "1.1.0", minHelperVersion: "1.0.6", backendReady: false})
  state.handleEvent({event: "ready", helperVersion: ""})
  assert.equal(state.backendVersionMismatch, true)
}

{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.1.0", backendReady: false,
    backendVersionMismatch: true, errorText: "Nearby needs helper 1.1.0. Installed: 1.0.7.",
    backend: {running: false, write: () => {}},
  })
  state.root = state

  state.startHelperUpdate()
  assert.equal(state.helperUpdating, true)
  assert.equal(state.helperUpdater.running, true)
  state.startHelperUpdate()
  assert.equal(state.helperUpdater.launched, false,
    "a second press while the updater runs must not start another download")

  state.handleUpdaterEvent({event: "step", message: "Downloading helper v1.1.0…"})
  assert.equal(state.helperUpdateStatus, "Downloading helper v1.1.0…")

  state.handleUpdaterEvent({event: "done", version: "1.1.0"})
  state.finishHelperUpdate(0)
  assert.equal(state.helperUpdating, false)
  assert.equal(state.backendVersionMismatch, false)
  assert.equal(state.helperUpdateError, "")
  assert.equal(state.errorText, "")
  assert.equal(typeof state.backend.running.callback, "function",
    "the restored binding is what starts the helper the update just installed")
  assert.equal(state.backend.running.callback(), true)
}

// A failed prefetch leaves the previous cache state in place, so the plugin
// must stay in the state that says so rather than pretending it is repaired.
{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.1.0", backendReady: false,
    backendVersionMismatch: true, backend: {running: false, write: () => {}},
  })
  state.root = state
  state.startHelperUpdate()
  state.handleUpdaterEvent({event: "failed", message: "Download of the v1.1.0 helper failed"})
  state.finishHelperUpdate(1)
  assert.equal(state.helperUpdating, false)
  assert.equal(state.helperUpdateError, "Download of the v1.1.0 helper failed")
  assert.equal(state.backendVersionMismatch, true)
  assert.equal(state.backend.running, false, "a failed update must not try to start the old helper again")
}

// Preserve the historical tolerance for a late process exit after a reported
// success. The verified cache is authoritative once the done event arrives.
{
  const state = engine({
    pluginVersion: "1.1.0", minHelperVersion: "1.1.0", backendReady: false,
    backendVersionMismatch: true, backend: {running: false, write: () => {}},
  })
  state.root = state
  state.startHelperUpdate()
  state.handleUpdaterEvent({event: "done", version: "1.1.0"})
  state.finishHelperUpdate(143)
  assert.equal(state.helperUpdateError, "")
  assert.equal(state.backendVersionMismatch, false)
  assert.equal(typeof state.backend.running.callback, "function")
}

// An updater that dies without reporting anything still has to say something.
{
  const state = engine({pluginVersion: "1.1.0", backend: {running: false, write: () => {}}})
  state.root = state
  state.startHelperUpdate()
  state.finishHelperUpdate(1)
  assert.equal(state.helperUpdateError, "Nearby could not update the helper.")
}

function incoming(requestId, sender = requestId) {
  return {event: "incoming_request", requestId, sender, files: [{name: `${requestId}.txt`}], total: 1}
}

{
  const state = engine({viewState: "nearby"})
  state.handleEvent({event: "incoming_pin_state", enabled: false})
  assert.equal(state.incomingPinEnabled, false)
  state.openIncomingPinSettings()
  assert.equal(state.viewState, "incoming_pin_settings")
  state.beginIncomingPinEdit()
  assert.equal(state.viewState, "incoming_pin_edit")

  state.submitIncomingPin("bad pin")
  assert.equal(state.sent.some(command => command.command === "set_incoming_pin"), false)
  assert.match(state.incomingPinError, /1–64/)
  assert.ok(state.signals.incomingPinFocusRequested > 0)

  state.submitIncomingPin("Abc-_.~09")
  assert.deepEqual(state.sent.at(-1), {command: "set_incoming_pin", pin: "Abc-_.~09"})
  assert.equal(state.incomingPinUpdating, true)
  assert.equal(Object.hasOwn(state, "incomingPin"), false,
    "the service must never retain a readable copy of the submitted PIN")
  state.handleEvent({event: "incoming_pin_state", enabled: true})
  assert.equal(state.incomingPinEnabled, true)
  assert.equal(state.incomingPinUpdating, false)
  assert.equal(state.viewState, "incoming_pin_settings")
  assert.ok(state.signals.incomingPinCleared > 0)

  state.requestDisableIncomingPin()
  assert.equal(state.viewState, "incoming_pin_disable")
  state.confirmDisableIncomingPin()
  assert.deepEqual(state.sent.at(-1), {command: "disable_incoming_pin"})
  state.handleEvent({event: "incoming_pin_state", enabled: false})
  assert.equal(state.incomingPinEnabled, false)
}

{
  const state = engine({viewState: "incoming_pin_edit"})
  state.submitIncomingPin("Safe-1")
  state.handleEvent({event: "incoming_pin_update_failed", message: "Unable to update incoming PIN"})
  assert.equal(state.incomingPinUpdating, false)
  assert.equal(state.pendingIncomingPinEnabled, null)
  assert.equal(state.incomingPinError, "Unable to update incoming PIN")
  assert.ok(state.signals.incomingPinCleared > 0)
}

{
  const state = engine({
    backendReady: false,
    viewState: "incoming_pin_settings",
    incomingPinEnabled: true,
  })
  state.beginIncomingPinEdit()
  state.requestDisableIncomingPin()
  state.submitIncomingPin("Safe-1")
  state.confirmDisableIncomingPin()
  assert.equal(state.viewState, "incoming_pin_settings")
  assert.equal(state.incomingPinUpdating, false)
  assert.equal(state.sent.length, 0,
    "receiver security controls must not queue an update while the helper is unavailable")
}

{
  const state = engine({viewState: "incoming_pin_edit"})
  const before = state.signals.incomingPinCleared
  state.handleEvent(incoming("authenticated"))
  assert.equal(state.viewState, "incoming")
  assert.ok(state.signals.incomingPinCleared > before,
    "an incoming request that preempts PIN settings must clear the local secret field")
}

{
  const state = engine({viewState: "incoming_pin_edit"})
  state.submitIncomingPin("Safe-1")
  state.handleEvent(incoming("raced"))
  state.handleEvent({event: "incoming_pin_state", enabled: true})
  assert.equal(state.viewState, "incoming",
    "a late PIN-save confirmation must not hide an incoming transfer that preempted settings")
  assert.equal(state.incoming.requestId, "raced")
}

{
  const state = engine({viewState: "incoming_pin_edit"})
  const before = state.signals.incomingPinCleared
  state.handleEvent({event: "incoming_text", sessionId: "text", sender: "Alice", text: "hello"})
  assert.equal(state.viewState, "text")
  assert.ok(state.signals.incomingPinCleared > before,
    "incoming text that preempts PIN settings must clear the local secret field")
}

// A desktop notification exists to reach a user who is not looking at the
// panel. Raised while the panel is open on the very request it announces, it is
// a duplicate the user has to dismiss on top of answering the prompt.
{
  const state = engine({viewState: "nearby", openViewCount: 1, anyViewOpen: true})
  state.handleEvent(incoming("visible"))
  assert.equal(state.viewState, "incoming")
  assert.deepEqual(notificationTitles(state), [],
    "a panel already showing the request must not also raise a notification")
}

{
  const state = engine({viewState: "nearby", openViewCount: 0, anyViewOpen: false})
  state.handleEvent(incoming("unseen", "Alice"))
  assert.deepEqual(notificationCommands(state), [[
    "omarchy-notification-send", "--app-name", "Nearby",
    "--urgency", "normal",
    "Incoming transfer", "Alice wants to send unseen.txt",
  ]], "an incoming transfer must keep its identity and contents through Omarchy's notification helper")
  assert.deepEqual(notificationTitles(state), ["Incoming transfer"],
    "a closed panel is the case the notification exists for")
}

// Open is not the same as displayed. A request that arrives while the panel is
// busy is queued rather than shown, so going quiet would drop it silently.
for (const busy of ["sending", "receiving", "pin"]) {
  const state = engine({viewState: busy, anyViewOpen: true})
  state.handleEvent(incoming("held"))
  assert.equal(state.viewState, busy)
  assert.deepEqual(notificationTitles(state), ["Incoming transfer"],
    `a request held behind ${busy} is not on screen and still has to be announced`)
}

// The panel shows the head of the queue, so a second request is not visible
// either even though the view is already the incoming one.
{
  const state = engine({viewState: "nearby", anyViewOpen: true})
  state.handleEvent(incoming("first"))
  state.handleEvent(incoming("second"))
  assert.equal(state.incoming.requestId, "first")
  assert.deepEqual(notificationTitles(state), ["Incoming transfer"],
    "only the request queued behind the displayed one may notify")
}

// Received text follows the same rule.
{
  const state = engine({viewState: "nearby", anyViewOpen: true})
  state.handleEvent({event: "incoming_text", sender: "Alice", text: "hello"})
  assert.equal(state.viewState, "text")
  assert.deepEqual(notificationTitles(state), [],
    "a panel already showing the text must not also raise a notification")
}

{
  const state = engine({viewState: "nearby", anyViewOpen: false})
  state.handleEvent({event: "incoming_text", sender: "Alice", text: "hello"})
  assert.deepEqual(notificationCommands(state), [[
    "omarchy-notification-send", "--app-name", "Nearby",
    "--urgency", "normal",
    "Text received", "From Alice",
  ]], "received text must keep its identity and contents through Omarchy's notification helper")
  assert.deepEqual(notificationTitles(state), ["Text received"])
}

// Its held case is an outgoing transfer in flight: the text is kept until the
// send finishes instead of being shown, so the open panel does not cover it.
{
  const state = engine({
    viewState: "sending",
    anyViewOpen: true,
    pendingOutgoing: {kind: "text", device: {fingerprint: "phone"}, text: "outgoing"},
  })
  state.handleEvent({event: "incoming_text", sender: "Alice", text: "deferred"})
  assert.equal(state.incomingTextPending, true)
  assert.deepEqual(notificationTitles(state), ["Text received"],
    "text deferred behind an outgoing transfer is not on screen and still has to be announced")
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
  const state = engine({viewState: "incoming_pin_edit"})
  state.handleBackendExit(1)
  assert.equal(state.viewState, "error",
    "helper exit must not leave receiver security controls connected to nothing")
  assert.match(state.errorText, /receiver security/i)
}

{
  // A helper that exits before announcing itself still uses the bounded retry
  // schedule. Its cause is independent: an early exit is not, by itself,
  // evidence that the LocalSend port is occupied.
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
  assert.match(state.errorText, /could not start/i)
  assert.doesNotMatch(state.errorText, /53317|LocalSend/i,
    "an unknown pre-ready failure must remain generic")
}

{
  const restarts = []
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 0, interval: 0, restart() { restarts.push(this.attempts) }, stop: () => {}},
  })
  state.handleEvent({event: "startup_failed", code: "receiver_port_in_use", port: 53317})
  assert.equal(state.backendStartupFailureCode, "receiver_port_in_use")
  assert.equal(state.backendStartupFailurePort, 53317)
  state.handleBackendExit(1)
  assert.equal(state.backendRestart.attempts, 1,
    "a confirmed port conflict must preserve bounded retries")
  assert.deepEqual(restarts, [1])
}

{
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 4, interval: 0, restart: () => { throw new Error("must not retry") }, stop: () => {}},
  })
  state.handleEvent({event: "startup_failed", code: "receiver_port_in_use", port: 53317})
  state.handleBackendExit(1)
  assert.match(state.errorText, /53317/,
    "a confirmed port conflict that outlasts retries must name the port")
  assert.match(state.errorText, /another LocalSend receiver/i)
  assert.match(state.errorText, /quit it|close it/i,
    "the final message must tell the user how to release the port")
}

{
  const restarts = []
  const state = engine({
    backendAcceptedThisRun: false,
    backendRestart: {attempts: 0, interval: 0, restart() { restarts.push(true) }, stop: () => {}},
  })
  state.handleEvent({event: "startup_failed", code: "receiver_security_settings_invalid"})
  state.handleBackendExit(1)
  assert.match(state.errorText, /security settings are invalid/i)
  assert.match(state.errorText, /settings\.json/)
  assert.match(state.errorText, /XDG state directory/i)
  assert.deepEqual(restarts, [],
    "a persistent security configuration error must fail closed without a retry loop")
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
  // The startup window itself: the service exists before shell.json has been
  // applied, so eligibility to run has to be false then, not merely corrected
  // afterwards. Evaluating the binding is the only way to cover the window --
  // checking receiverEnabled after the fact would pass even if the helper had
  // already bound the port.
  assert.equal(backendEligible({receiverConfigured: false, receiverEnabled: true, pluginVersion: "1.0.6-dev"}), false,
    "the helper must not be eligible to start before shell.json has been read")
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: false, pluginVersion: "1.0.6-dev"}), false,
    "a persisted receiverEnabled: false must keep the helper stopped")
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: true, pluginVersion: ""}), false,
    "the helper must not start before its version is known")
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: true, pluginVersion: "1.0.6-dev"}), true)
}

{
  // Replaying the real startup order: the config arrives after the service is
  // built, and a persisted off must never pass through an eligible state.
  const off = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: false}]}}, plugins: []}
  const observed = []
  for (const config of [null, {version: 1, bar: {layout: {right: []}}, plugins: []}, off]) {
    const entry = Model.barEntry(config, "oma.nearby")
    observed.push(backendEligible({
      receiverConfigured: entry !== null,
      receiverEnabled: Model.receiverEnabledIn(entry),
      pluginVersion: "1.0.6-dev",
    }))
  }
  assert.deepEqual(observed, [false, false, false],
    "a persisted off receiver must not become eligible at any point during startup")
}

{
  // The persisted value and the effective value are one value, so they cannot
  // diverge: there is nothing to synchronize and no snapshot to go stale.
  const config = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: false}]}}, plugins: []}
  const state = engine({shell: {shellConfig: config, updateEntryInline: () => true}})
  const entry = Model.barEntry(config, "oma.nearby")
  assert.equal(Model.receiverEnabledIn(entry), false)
  assert.deepEqual(entry.settings, {receiverEnabled: false})
  assert.equal(entry.id, "oma.nearby")
  // Turning the receiver on writes the entry rather than a second copy of the
  // state, so a later read of the config is what makes it effective.
  const writes = []
  // Objects built inside the vm realm carry that realm's prototype, so they
  // are compared by value.
  state.shell = {shellConfig: config, updateEntryInline: (id, settings) => { writes.push([id, JSON.parse(JSON.stringify(settings))]); return true }}
  state.persistReceiverEnabled(true)
  assert.deepEqual(writes, [["oma.nearby", {receiverEnabled: true}]],
    "toggling must persist the entry, which is the only place the state lives")
}

{
  // Other keys on the entry survive a receiver toggle.
  const config = {version: 1, bar: {layout: {right: [{id: "oma.nearby", tooltip: "keep me"}]}}, plugins: []}
  const entry = Model.barEntry(config, "oma.nearby")
  const writes = []
  const state = engine({
    pluginSettings: entry.settings,
    moduleEntryId: entry.id,
    shell: {shellConfig: config, updateEntryInline: (id, settings) => { writes.push(JSON.parse(JSON.stringify(settings))); return true }},
  })
  state.persistReceiverEnabled(false)
  assert.deepEqual(writes, [{tooltip: "keep me", receiverEnabled: false}],
    "persisting the receiver must not drop unrelated inline settings")
}

{
  // Quattro accepts string-form entries in bar.layout, but its existing
  // updateEntryInline() only matches objects through entry.id. Persisting from
  // a string must therefore promote that exact slot to object form rather than
  // silently leaving the authoritative receiver state on.
  const config = {
    version: 1,
    bar: {layout: {right: ["other.before", "oma.nearby", {id: "other.after", x: 1}]}},
    plugins: [],
  }
  const shell = {
    shellConfig: config,
    updateEntryInline(id, settings) {
      const entries = this.shellConfig.bar.layout.right
      const index = entries.findIndex(entry => entry && entry.id === id)
      if (index < 0) return false
      entries[index] = {id, ...settings}
      return true
    },
    mutateShellConfig(mutator) {
      const copy = JSON.parse(JSON.stringify(this.shellConfig))
      mutator(copy)
      this.shellConfig = copy
    },
  }
  const state = engine({
    shell,
    moduleEntryId: "oma.nearby",
    pluginSettings: {},
  })

  state.persistReceiverEnabled(false)
  assert.deepEqual(shell.shellConfig.bar.layout.right,
    ["other.before", {id: "oma.nearby", receiverEnabled: false}, {id: "other.after", x: 1}],
    "persisting off must promote the matching string in place without changing its neighbors")
  const offEntry = Model.barEntry(shell.shellConfig, "oma.nearby")
  assert.equal(Model.receiverEnabledIn(offEntry), false)
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: false, pluginVersion: "1.0.6-dev"}), false)

  state.pluginSettings = offEntry.settings
  state.persistReceiverEnabled(true)
  const onEntry = Model.barEntry(shell.shellConfig, "oma.nearby")
  assert.equal(Model.receiverEnabledIn(onEntry), true)
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: true, pluginVersion: "1.0.6-dev"}), true)
  assert.equal(shell.shellConfig.bar.layout.right.filter(entry =>
    entry === "oma.nearby" || (entry && entry.id === "oma.nearby")).length, 1,
    "promoting and toggling a string-form entry must not create a duplicate")
}

// Omarchy 4.0.3+ injects a scoped PluginShellApi that no longer carries
// shellConfig. The fallback file is what the receiver reads, and a valid one
// must not permanently resolve the toggle to off.
{
  const state = engine({
    fileShellConfig: {version: 1, bar: {layout: {right: [{id: "oma.nearby"}]}}, plugins: []},
    shell: {},
  })
  assert.equal(Object.hasOwn(state.shell, "shellConfig"), false,
    "the scoped shell test fixture must model the missing shellConfig")
  assert.equal(Model.receiverEnabledIn(configEntry(state)), true,
    "a valid fallback entry without a receiver setting means on, like the injected config")
}
{
  const state = engine({
    fileShellConfig: {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: false}]}}, plugins: []},
    shell: {},
  })
  assert.equal(Model.receiverEnabledIn(configEntry(state)), false,
    "the fallback file must keep a persisted off the source the toggle reads")
}

// A legacy host that still injects the real shell.shellConfig keeps using it:
// the file must never override the live injected config.
{
  const legacy = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: true}]}}, plugins: []}
  const fileOff = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: false}]}}, plugins: []}
  const state = engine({shell: {shellConfig: legacy}, fileShellConfig: fileOff})
  assert.equal(state.hasLegacyShellConfig, true)
  assert.equal(Model.receiverEnabledIn(configEntry(state)), true,
    "a real injected shellConfig stays authoritative on legacy hosts")
}

// Config reload: the watched file is what a modern shell update flips, and a
// restart reads the file again, so the persisted value must become effective.
{
  const state = engine({fileShellConfig: {version: 1, bar: {layout: {right: []}}, plugins: []}, shell: {}})
  assert.equal(configEntry(state), null, "a file that does not contain the entry keeps the receiver off")
  state.fileShellConfig = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: true}]}}, plugins: []}
  assert.equal(Model.receiverEnabledIn(configEntry(state)), true,
    "a refreshed fallback must make the persisted receiver state effective")
}

// OFF -> ON on a scoped shell must persist through the self-scoped
// updateEntryInline(), and a mutateShellConfig() that only exists as a denied
// capability must never be called or swallow the write.
{
  const config = {version: 1, bar: {layout: {right: [{id: "oma.nearby", receiverEnabled: false}]}}, plugins: []}
  const writes = []
  const mutates = []
  const state = engine({
    fileShellConfig: config,
    moduleEntryId: "oma.nearby",
    pluginSettings: {receiverEnabled: false},
    shell: {
      updateEntryInline(id, settings) { writes.push([id, JSON.parse(JSON.stringify(settings))]); return true },
      mutateShellConfig() { mutates.push(true); return false },
    },
  })
  state.persistReceiverEnabled(true)
  assert.deepEqual(writes, [["oma.nearby", {receiverEnabled: true}]],
    "a scoped shell must persist through the self-scoped updateEntryInline()")
  assert.deepEqual(mutates, [],
    "a denied mutateShellConfig() must never capture the toggle and drop the write")
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: true, pluginVersion: "1.0.6-dev"}), true,
    "the persisted ON must leave the helper eligible to start")
}

// ON -> OFF on a scoped shell preserves the entry's unrelated settings.
{
  const config = {version: 1, bar: {layout: {right: [{id: "oma.nearby", tooltip: "keep me", receiverEnabled: true}]}}, plugins: []}
  const writes = []
  const state = engine({
    fileShellConfig: config,
    moduleEntryId: "oma.nearby",
    pluginSettings: {tooltip: "keep me", receiverEnabled: true},
    shell: {updateEntryInline(id, settings) { writes.push(JSON.parse(JSON.stringify(settings))); return true }},
  })
  state.persistReceiverEnabled(false)
  assert.deepEqual(writes, [{tooltip: "keep me", receiverEnabled: false}],
    "an OFF write on a scoped shell must preserve unrelated inline settings")
  assert.equal(backendEligible({receiverConfigured: true, receiverEnabled: false, pluginVersion: "1.0.6-dev"}), false)
}

// No unguarded shell.shellConfig read may remain in the service: the only
// allowed reads sit behind hasLegacyShellConfig as the legacy compatibility
// path.
assert.doesNotMatch(source, /Model\.barEntry\(shell\.shellConfig/,
  "configEntry must derive from the compatibility source, not shell.shellConfig")
assert.doesNotMatch(source, /Model\.hasStringBarEntry\(shell\.shellConfig/,
  "string promotion must be decided from the compatibility source")
assert.match(source, /readonly property bool hasLegacyShellConfig:\s*!!shell && !!shell\.shellConfig/,
  "the legacy-host discriminator must require a truthy shell.shellConfig")
assert.match(source, /readonly property var effectiveShellConfig:\s*hasLegacyShellConfig \? shell\.shellConfig : fileShellConfig/,
  "the compatibility source must prefer the live injected config when present")
assert.match(source, /watchChanges:\s*true/,
  "the fallback shell.json watcher must track the shell's atomic rewrites")
assert.match(source, /onFileChanged:\s*reload\(\)/,
  "the watcher must refresh after the shell persists a settings update")

assert.match(source,
  /function failWith\(message\)\s*\{\s*viewState="error";\s*errorText=message;\s*statusText=errorText\s*\}/,
  "failWith must keep statusText aligned with errorText for every failure that reaches it")

console.log("service state tests passed")
