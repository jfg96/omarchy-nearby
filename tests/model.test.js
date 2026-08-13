const assert = require("node:assert/strict")
const Model = require("../Model.js")

const phone = {alias:"<b>Phone</b>",fingerprint:"fp",deviceType:"mobile",ip:"192.0.2.2"}
assert.equal(Model.upsertDevice([], phone).length, 1)
assert.equal(Model.upsertDevice(Model.upsertDevice([], phone), {...phone, alias:"Phone 2"})[0].alias, "Phone 2")
assert.deepEqual(Model.snapshotDevices([phone, {...phone, alias:"Newest"}]).map(d => d.alias), ["Newest"])
assert.equal(Model.parseLine("not json"), null)
const requestA = {requestId:"a",sender:"Alice"}
const requestB = {requestId:"b",sender:"Bob"}
const requestC = {requestId:"c",sender:"Carol"}
assert.deepEqual(Model.enqueueIncoming([], null), [])
assert.deepEqual(Model.enqueueIncoming([], {}), [])
assert.deepEqual(Model.enqueueIncoming([], requestA), [requestA])
assert.deepEqual(Model.enqueueIncoming([requestA], requestB), [requestA, requestB])
assert.deepEqual(Model.enqueueIncoming([requestA, requestB], {...requestA, sender:"Alice updated"}).map(r => r.sender), ["Alice updated", "Bob"])
assert.deepEqual(Model.removeIncoming([requestA, requestB, requestC], "b"), [requestA, requestC])
assert.deepEqual(Model.removeIncoming([requestA, requestB], "missing"), [requestA, requestB])
assert.equal(Model.currentIncoming([requestA, requestB]), requestA)
assert.equal(Model.currentIncoming([]), null)
assert.equal(Model.currentIncoming(null), null)
const pendingFiles = {kind:"files",device:phone,paths:["/tmp/a","/tmp/b"]}
const pendingText = {kind:"text",device:phone,text:"hello"}
assert.equal(Model.outgoingCommand(null, "out-1", null), null)
assert.deepEqual(Model.outgoingCommand(pendingFiles, "out-1", null), {command:"send_files",transfer_id:"out-1",device:phone,paths:["/tmp/a","/tmp/b"]})
assert.deepEqual(Model.outgoingCommand(pendingText, "out-2", "123456"), {command:"send_text",transfer_id:"out-2",device:phone,text:"hello",pin:"123456"})
assert.equal(Object.hasOwn(pendingText, "pin"), false)
assert.equal(Model.viewAfterOutgoing(true, "success"), "incoming")
assert.equal(Model.viewAfterOutgoing(false, "error"), "error")
assert.equal(Model.helperVersionMatches("1.0.0", "1.0.0"), true)
assert.equal(Model.helperVersionMatches("1.0.1-dev", "1.0.0"), false)
assert.equal(Model.helperVersionMatches("1.0.0", ""), false)
assert.equal(Model.helperVersionMatches("", ""), false)
assert.equal(Model.manifestVersion('{"id":"oma.nearby","version":"1.0.2"}', "oma.nearby"), "1.0.2")
assert.equal(Model.manifestVersion('{"id":"other.plugin","version":"1.0.2"}', "oma.nearby"), "")
assert.equal(Model.manifestVersion('{"id":"oma.nearby"}', "oma.nearby"), "")
assert.equal(Model.manifestVersion("not json", "oma.nearby"), "")
// The service reads its own entry out of shell.json. An absent entry is a
// config that has not been applied yet, which is why it is null rather than an
// empty settings object: the receiver stays off until the entry is seen, so a
// persisted off is never briefly treated as on.
const layoutOff = {version:1, bar:{layout:{left:[{id:"omarchy.menu"}], center:[], right:[{id:"b.omadoro"},{id:"oma.nearby",receiverEnabled:false}]}}, plugins:[]}
assert.deepEqual(Model.barEntry(layoutOff, "oma.nearby"), {id:"oma.nearby", settings:{receiverEnabled:false}})
assert.equal(Model.receiverEnabledIn(Model.barEntry(layoutOff, "oma.nearby")), false)

const layoutDefault = {version:1, bar:{layout:{right:[{id:"oma.nearby"}]}}, plugins:[]}
assert.deepEqual(Model.barEntry(layoutDefault, "oma.nearby"), {id:"oma.nearby", settings:{}})
assert.equal(Model.receiverEnabledIn(Model.barEntry(layoutDefault, "oma.nearby")), true,
  "an entry with no receiver setting means on, the way it always has")

assert.equal(Model.barEntry(layoutDefault, "other.plugin"), null)
assert.equal(Model.barEntry({version:1, bar:{layout:{right:[]}}, plugins:[]}, "oma.nearby"), null)
assert.equal(Model.barEntry(null, "oma.nearby"), null)
assert.equal(Model.barEntry(undefined, "oma.nearby"), null)
assert.equal(Model.barEntry({}, "oma.nearby"), null)
assert.equal(Model.barEntry(layoutDefault, ""), null)
assert.equal(Model.receiverEnabledIn(null), false,
  "no entry means the receiver is not eligible to run yet, not that it defaults to on")

// Non-widget plugin entries live in plugins[] instead of the bar layout.
assert.deepEqual(Model.barEntry({version:1, plugins:[{id:"oma.nearby",receiverEnabled:false}]}, "oma.nearby"),
  {id:"oma.nearby", settings:{receiverEnabled:false}})
// Malformed entries must not throw or match.
assert.equal(Model.barEntry({version:1, bar:{layout:{right:[null,"oma.nearby",42]}}}, "oma.nearby"), null)

console.log("Model tests passed")
