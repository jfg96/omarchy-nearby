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
console.log("Model tests passed")
