const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const Model = require("../Model.js")

const source = fs.readFileSync(require.resolve("../Panel.qml"), "utf8")

// The bar builds one widget per monitor (Variants over Quickshell.screens).
// Anything there can only be one of belongs to the service, which the shell
// loads once. When the widget owned the helper, the second monitor's copy lost
// the race for the LocalSend port and reported a receiver that could not
// start while the first copy was holding it.
assert.equal(source.includes("omarchy-nearby-helper"), false,
  "the widget must not spawn the helper: one would be started per monitor")
assert.equal(source.includes("IpcHandler"), false,
  "the widget must not register an IPC target: one handler would be registered per monitor")
assert.match(source, /bar\.shell\.serviceFor\(manifestPluginId\)/,
  "the widget must read its state from the single service instance")
assert.match(source, /manageIpc:\s*false/,
  "the base panel's IPC handler must stay off so the service owns the target")
assert.equal(source.includes("Qt.ImhDigitsOnly"), false,
  "the outgoing PIN prompt must accept the same text values as LocalSend")
assert.equal(source.includes("RegularExpressionValidator { regularExpression: /[0-9]*/ }"), false,
  "the outgoing PIN prompt must not silently reject non-numeric PINs")
assert.doesNotMatch(source, /id:\s*pinInput[^\n]*maximumLength:\s*32/,
  "the outgoing PIN prompt must not retain its old 32-character limit")
for (const owned of ["backendRestart", "receiverShutdownFallback", "handleEvent(", "handleBackendExit("]) {
  assert.equal(source.includes(owned), false,
    `${owned} is engine state and must not be duplicated per monitor`)
}

// Cursor position and popup focus are genuinely per monitor and stay here.
assert.match(source, /onCursorRequested\(index\)\s*\{\s*root\.selectedIndex = index\s*\}/,
  "the engine asks each view to move its own cursor rather than holding one")
assert.match(source, /onOpenedChanged[\s\S]*?if \(root\.viewState==="pin"\) pinInput\.forceActiveFocus\(\)/,
  "reopening a pending PIN prompt must restore focus to its input")
assert.match(source, /Component\.onDestruction:\s*if \(engine && viewRegistered\) engine\.viewClosed\(\)/,
  "a view torn down while open must release its claim on discovery")
assert.match(source, /viewState\.indexOf\("incoming_pin_"\) === 0\) return "Receiver security"/,
  "PIN screens must not inherit stale discovery status in their header")
assert.equal((source.match(/text:"Back"/g) || []).length, 2,
  "the device actions and PIN settings screens both need a visible mouse exit")

assert.equal(source.includes("closeStdin"), false, "Quickshell Process does not expose closeStdin")
assert.match(source, /onStarted:\s*\{[^}]*write\(root\.incomingText\);\s*stdinEnabled=false\s*\}/,
  "clipboard writer must close stdin after writing so wl-copy can finish")
assert.match(source, /onExited:\s*function\(code\)\s*\{\s*stdinEnabled=true;/,
  "clipboard writer must re-enable stdin for the next copy")
assert.match(source, /onExited: function\(code\) \{ stdinEnabled=true; if\(root\.viewState!=="text"\)return;/,
  "late wl-copy completion must not reopen a text result after the user exits")

assert.equal(source.includes("zenity"), false,
  "files are chosen with omarchy-file-select, which Omarchy ships, rather than zenity")
assert.match(source, /command:\s*\["omarchy-file-select"/,
  "the file chooser must be omarchy-file-select")
assert.equal(/code\s*===?\s*127/.test(source), false,
  "no shell runs on our behalf, so a missing command never reports exit code 127")
for (const launcher of ["picker", "clipboard", "clipboardWriter"]) {
  assert.match(source, new RegExp(`onRunningChanged:\\s*if\\s*\\(!running && !${launcher}\\.launched`),
    `${launcher} must report a command that never launched, which Quickshell signals by ` +
    "returning running to false without an exit code")
}

for (const [failure, description] of [
  ["Clipboard is empty", "clipboard read"],
  ["wl-paste is required to read the clipboard", "missing wl-paste"],
  ["wl-copy is required to copy received text", "clipboard write"],
  ["The file chooser could not be started", "missing file chooser"],
  ["The file chooser did not open", "file chooser fault"],
]) {
  assert.match(source, new RegExp(`failWith\\("${failure.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"\\)`),
    `${description} errors must be raised through failWith`)
}

function extractFunction(name) {
  const marker = `function ${name}`
  const start = source.indexOf(marker)
  assert.notEqual(start, -1, `Panel.qml must define ${name}`)
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
  "syncOpenState", "toggleReceiver", "startDiscovery", "forceFullDiscovery",
  "chooseDevice", "acceptIncoming", "declineIncoming", "finishText", "finishTerminal",
  "cancelOutgoing", "cancelPin", "retryWithPin", "failWith", "noteTextCopied", "beginOutgoing",
  "openIncomingPinSettings", "beginIncomingPinEdit", "requestDisableIncomingPin",
  "cancelIncomingPinSettings", "submitIncomingPin", "confirmDisableIncomingPin",
  "goBack", "selectFiles", "sendClipboard", "copyReceivedText", "moveCursor", "activateCursor",
]

// Every call the view makes has to land on the shared engine, so the stub
// records rather than implements.
function stubEngine() {
  const calls = []
  const record = name => (...args) => { calls.push([name, ...args]); return undefined }
  return {
    calls,
    toggleReceiver: record("toggleReceiver"),
    startDiscovery: record("startDiscovery"),
    forceFullDiscovery: record("forceFullDiscovery"),
    chooseDevice: record("chooseDevice"),
    clearTarget: record("clearTarget"),
    acceptIncoming: record("acceptIncoming"),
    declineIncoming: record("declineIncoming"),
    finishText: record("finishText"),
    finishTerminal: record("finishTerminal"),
    cancelOutgoing: record("cancelOutgoing"),
    cancelPin: record("cancelPin"),
    retryWithPin: record("retryWithPin"),
    openIncomingPinSettings: record("openIncomingPinSettings"),
    beginIncomingPinEdit: record("beginIncomingPinEdit"),
    requestDisableIncomingPin: record("requestDisableIncomingPin"),
    cancelIncomingPinSettings: record("cancelIncomingPinSettings"),
    submitIncomingPin: record("submitIncomingPin"),
    confirmDisableIncomingPin: record("confirmDisableIncomingPin"),
    failWith: record("failWith"),
    noteTextCopied: record("noteTextCopied"),
    beginOutgoing: record("beginOutgoing"),
    viewOpened: record("viewOpened"),
    viewClosed: record("viewClosed"),
  }
}

function panel(initial = {}) {
  const closes = []
  const engine = initial.engine === undefined ? stubEngine() : initial.engine
  const context = {
    Model,
    Qt: {callLater: callback => callback()},
    engine,
    picker: {running: false, launched: false},
    clipboard: {running: false, launched: false},
    clipboardWriter: {running: false, launched: false},
    pinInput: {text: "", forceActiveFocus: () => {}},
    incomingPinInput: {text: "", forceActiveFocus: () => {}},
    keyCatcher: {forceActiveFocus: () => {}},
    close: () => closes.push(true),
    moduleName: "oma.nearby",
    settings: {},
    opened: true,
    viewRegistered: false,
    selectedIndex: 0,
    cursorActive: false,
    devices: [],
    selectedDevice: {fingerprint: "phone", alias: "Phone"},
    viewState: "target",
    receiverEnabled: true,
    backendReady: true,
    incomingPinEnabled: false,
    ...initial,
  }
  context.engine = engine
  vm.createContext(context)
  vm.runInContext(functionNames.map(extractFunction).join("\n"), context)
  context.closes = closes
  return context
}

function callNames(state) {
  return state.engine.calls.map(call => call[0])
}

{
  const state = panel({viewState: "nearby", devices: [{alias: "A"}, {alias: "B"}], selectedIndex: 0})
  state.activateCursor()
  assert.deepEqual(state.engine.calls.at(-1), ["chooseDevice", 0])
  state.selectedIndex = 2
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "forceFullDiscovery",
    "the entry past the last device is the rescan action")
  state.selectedIndex = -1
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "toggleReceiver",
    "the cursor above the list is the receiver switch")
  state.selectedIndex = 3
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "openIncomingPinSettings",
    "the entry after rescan opens incoming PIN settings")
}

{
  const state = panel({viewState: "incoming", selectedIndex: 1})
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "acceptIncoming")
  state.selectedIndex = 0
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "declineIncoming")
}

{
  const state = panel({viewState: "sending"})
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "cancelOutgoing")
}

{
  const state = panel({viewState: "success"})
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "finishTerminal",
    "keyboard Done must reach the engine the same way the visual button does")
}

{
  const state = panel({viewState: "text", selectedIndex: 1})
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "finishText",
    "received text must have an exit independent of wl-copy completion")
}

{
  const state = panel({viewState: "nearby", devices: [{alias: "A"}, {alias: "B"}], selectedIndex: 0})
  state.moveCursor(0, 1)
  assert.equal(state.selectedIndex, 1)
  state.moveCursor(0, 5)
  assert.equal(state.selectedIndex, 3, "the cursor stops on incoming PIN settings after rescan")
  state.moveCursor(0, -9)
  assert.equal(state.selectedIndex, -1, "the cursor stops on the receiver switch above the list")
  assert.equal(state.engine.calls.length, 0, "moving a cursor is local to one monitor")
}

{
  const state = panel({viewState: "target"})
  state.goBack()
  assert.equal(callNames(state).at(-1), "clearTarget")
  const pin = panel({viewState: "pin"})
  pin.goBack()
  assert.equal(callNames(pin).at(-1), "cancelPin")
  const incomingPin = panel({viewState: "incoming_pin_settings"})
  incomingPin.goBack()
  assert.equal(callNames(incomingPin).at(-1), "cancelIncomingPinSettings")
  const nearby = panel({viewState: "nearby"})
  nearby.goBack()
  assert.equal(nearby.closes.length, 1, "back from the root view closes this monitor's popup")
  assert.equal(nearby.engine.calls.length, 0)
}

{
  const state = panel({viewState: "incoming_pin_edit"})
  state.incomingPinInput.text = "Abc-_.~09"
  state.submitIncomingPin()
  assert.deepEqual(state.engine.calls.at(-1), ["submitIncomingPin", "Abc-_.~09"])
}

{
  const disabled = panel({viewState: "incoming_pin_settings", incomingPinEnabled: false, selectedIndex: 0})
  disabled.activateCursor()
  assert.equal(callNames(disabled).at(-1), "beginIncomingPinEdit")
  const enabled = panel({viewState: "incoming_pin_settings", incomingPinEnabled: true, selectedIndex: 1})
  enabled.activateCursor()
  assert.equal(callNames(enabled).at(-1), "requestDisableIncomingPin")
  enabled.selectedIndex = 2
  enabled.activateCursor()
  assert.equal(callNames(enabled).at(-1), "cancelIncomingPinSettings",
    "enabled PIN settings must expose Back to keyboard users too")
  const disabledBack = panel({viewState: "incoming_pin_settings", incomingPinEnabled: false, selectedIndex: 1})
  disabledBack.activateCursor()
  assert.equal(callNames(disabledBack).at(-1), "cancelIncomingPinSettings",
    "disabled PIN settings must expose Back to keyboard users too")
  const confirmation = panel({viewState: "incoming_pin_disable", selectedIndex: 1})
  confirmation.activateCursor()
  assert.equal(callNames(confirmation).at(-1), "confirmDisableIncomingPin")
}

{
  const state = panel({viewState: "target", selectedIndex: 2})
  state.activateCursor()
  assert.equal(callNames(state).at(-1), "clearTarget",
    "device actions must expose Back to keyboard users too")
  state.selectedIndex = 0
  state.moveCursor(0, 9)
  assert.equal(state.selectedIndex, 2, "device action cursor must stop on Back")
}

{
  const state = panel()
  state.pinInput.text = "123456"
  state.retryWithPin()
  assert.deepEqual(state.engine.calls.at(-1), ["retryWithPin", "123456"],
    "the PIN is read from this monitor's field and handed to the shared engine")
}

{
  const state = panel({selectedDevice: null})
  state.selectFiles()
  state.sendClipboard()
  assert.equal(state.picker.running, false, "no chooser without a target device")
  assert.equal(state.clipboard.running, false, "no clipboard read without a target device")
  const ready = panel()
  ready.selectFiles()
  assert.equal(ready.picker.running, true)
  ready.sendClipboard()
  assert.equal(ready.clipboard.running, true)
}

{
  const state = panel({picker: {running: true, launched: true}})
  state.selectFiles()
  assert.equal(state.picker.launched, true, "a chooser already open must not be relaunched")
}

{
  // Discovery is engine state shared by every monitor, so a view registers and
  // releases its claim exactly once.
  const state = panel({opened: true, viewRegistered: false})
  state.syncOpenState()
  state.syncOpenState()
  assert.deepEqual(callNames(state), ["viewOpened"], "an open view claims discovery once")
  state.opened = false
  state.syncOpenState()
  assert.deepEqual(callNames(state), ["viewOpened", "viewClosed"])
}

{
  // The widget must not be the source of persisted settings. The bar injects
  // `settings` a tick after the widget is built (Bar.qml: onActiveItemChanged
  // -> Qt.callLater(injectProps)), so the first value a widget could report is
  // the default, not the persisted one -- which is how a persisted
  // receiverEnabled: false used to be replaced by the default on every start.
  assert.equal(source.includes("configure("), false,
    "the widget must not push settings into the engine; the engine reads shell.json")
  assert.equal(/onSettingsChanged/.test(source), false,
    "reacting to injected settings would reintroduce the widget as a config source")
}

{
  // A widget can outlive its engine across a plugin reload; it must degrade
  // rather than throw into the shell.
  const state = panel({engine: null, viewState: "target"})
  state.toggleReceiver()
  state.goBack()
  state.retryWithPin()
  state.failWith("boom")
  state.noteTextCopied()
  state.syncOpenState()
  assert.ok(true, "view actions without an engine must not throw")
}

console.log("panel state tests passed")
