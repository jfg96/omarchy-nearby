const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const Model = require("../Model.js")

const source = fs.readFileSync(require.resolve("../Panel.qml"), "utf8")

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
  "send", "persistReceiverEnabled", "stopDiscovery", "clearPendingOutgoing", "dispatchPendingOutgoing",
  "beginOutgoing", "retryWithPin", "showPinPrompt", "cancelPin",
  "acceptIncoming", "declineIncoming", "finishIncoming", "finishOutgoing", "finishReceiverShutdown", "handleEvent"
]

function panel(initial = {}) {
  const sent = []
  const context = {
    Model,
    Date: {now: () => 1000},
    Qt: {callLater: callback => callback()},
    Quickshell: {execDetached: () => {}},
    backend: {running: true, write: line => sent.push(JSON.parse(line))},
    pinInput: {text: "", forceActiveFocus: () => {}},
    keyCatcher: {forceActiveFocus: () => {}},
    backendRestart: {attempts: 0},
    receiverShutdownFallback: {stop: () => {}},
    settings: {},
    bar: null,
    moduleName: "oma.nearby",
    shutdownPending: false,
    pluginVersion: "1.0.4",
    backendVersionMismatch: false,
    backendReady: true,
    backendAcceptedThisRun: true,
    opened: true,
    receiverEnabled: true,
    discoveryActive: false,
    devices: [],
    selectedDevice: {fingerprint: "phone", alias: "Phone"},
    viewState: "target",
    statusText: "",
    errorText: "",
    selectedIndex: 0,
    cursorActive: false,
    incomingQueue: [],
    incomingText: "",
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
  return context
}

function incoming(requestId, sender = requestId) {
  return {event: "incoming_request", requestId, sender, files: [{name: `${requestId}.txt`}], total: 1}
}

{
  const state = panel()
  state.handleEvent(incoming("a"))
  state.handleEvent(incoming("b"))
  state.declineIncoming()
  assert.deepEqual(state.incomingQueue.map(item => item.requestId), ["b"])
  assert.equal(state.incoming.requestId, "b")
  assert.equal(state.viewState, "incoming")
}

{
  const state = panel()
  state.handleEvent(incoming("a"))
  state.handleEvent(incoming("b"))
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.equal(state.incoming.requestId, "b")
  assert.equal(state.viewState, "incoming")
  state.handleEvent({event: "incoming_expired", requestId: "unknown"})
  assert.equal(state.incoming.requestId, "b")
}

{
  const state = panel()
  state.handleEvent(incoming("a"))
  state.acceptIncoming()
  assert.equal(state.viewState, "receiving")
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.notEqual(state.viewState, "receiving",
    "an expired approval must not leave the UI waiting for a receive session that cannot start")
  assert.equal(state.activeIncomingSession, "")
}

{
  const state = panel()
  state.handleEvent(incoming("a"))
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "one"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_preparing", transferId, name: "message.txt", target: "Phone"})
  state.handleEvent({event: "outgoing_done", transferId})
  assert.equal(state.incoming.requestId, "a")
  assert.equal(state.viewState, "incoming")
}

{
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "original"})
  const firstId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId: firstId})
  state.handleEvent(incoming("a"))
  state.pinInput.text = "000000"
  state.retryWithPin()
  const retryId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_invalid_pin", transferId: retryId})
  state.handleEvent(incoming("b"))
  state.pinInput.text = "123456"
  state.retryWithPin()
  assert.equal(state.sent.at(-1).text, "original")
  assert.equal(state.sent.at(-1).pin, "123456")
  assert.deepEqual(state.incomingQueue.map(item => item.requestId), ["a", "b"])
  state.handleEvent({event: "incoming_expired", requestId: "a"})
  assert.equal(state.viewState, "pin")
  assert.equal(state.pendingOutgoing.text, "original")
  assert.equal(state.incoming.requestId, "b")
}

{
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "only once"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.pinInput.text = "123456"
  state.retryWithPin()
  state.pinInput.text = "123456"
  state.retryWithPin()
  assert.equal(state.sent.filter(command => command.pin === "123456").length, 1,
    "a repeated submit while a PIN retry is active must not dispatch a duplicate")
}

{
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "cancel retry"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  state.pinInput.text = "123456"
  state.retryWithPin()
  const retryId = state.outgoingTransferId
  state.cancelPin()
  assert.deepEqual(state.sent.filter(command => command.command === "cancel_outgoing"),
    [{command: "cancel_outgoing", transfer_id: retryId}],
    "leaving PIN while its retry is in flight must cancel that helper transfer exactly once")
  assert.equal(state.pendingOutgoing, null)
  assert.equal(state.outgoingTransferId, "")
}

{
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "keep me"})
  state.handleEvent({event: "outgoing_pin_required", transferId: state.outgoingTransferId})
  const pending = state.pendingOutgoing
  assert.match(source, /onOpenedChanged[\s\S]*?else\s*\{\s*stopDiscovery\(\)\s*\}/,
    "closing the popup must only stop discovery, leaving a PIN retry pending")
  assert.match(source, /onOpenedChanged[\s\S]*?if \(viewState==="pin"\) pinInput\.forceActiveFocus\(\)/,
    "reopening a pending PIN prompt must restore focus to its input")
  assert.equal(state.pendingOutgoing, pending)
}

{
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "pending"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId})
  state.handleEvent({event: "incoming_cancelled", sessionId: "unknown"})
  state.handleEvent({event: "incoming_failed", sessionId: "unknown", message: "late"})
  assert.equal(state.viewState, "pin")
  assert.equal(state.pendingOutgoing.text, "pending")
}

{
  const state = panel()
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
  const state = panel()
  state.beginOutgoing({kind: "text", device: state.selectedDevice, text: "outgoing"})
  const transferId = state.outgoingTransferId
  state.handleEvent({event: "outgoing_pin_required", transferId})
  state.handleEvent({event: "incoming_text", sessionId: "incoming-text", sender: "Alice", text: "hello"})
  assert.equal(state.viewState, "pin", "incoming text must not hide an unrelated PIN prompt")
  assert.ok(state.pendingOutgoing)
}

{
  const state = panel({viewState: "receiving"})
  state.handleEvent({event: "incoming_cancelled", sessionId: "session-a"})
  assert.equal(state.viewState, "error")
  assert.equal(state.activeIncomingSession, "")
  state.handleEvent({event: "incoming_done", sessionId: "session-a"})
  assert.equal(state.viewState, "error", "late terminal events must not resurrect a completed session")
}

{
  const state = panel()
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
  const state = panel()
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

console.log("panel state tests passed")
