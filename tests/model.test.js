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
assert.deepEqual(Model.outgoingCommand(pendingText, "out-3", "a+b & # % contraseña"),
  {command:"send_text",transfer_id:"out-3",device:phone,text:"hello",pin:"a+b & # % contraseña"})
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

// SemVer precedence, including the rule the release process depends on: a
// prerelease sorts below the release it leads up to, so a checkout on 1.1.0-dev
// is not satisfied by the 1.1.0 floor it is heading for and 1.1.0 is not held
// back by a 1.1.0-dev helper.
assert.equal(Model.compareVersions("1.0.0", "1.0.0"), 0)
assert.equal(Model.compareVersions("1.2.0", "1.10.0"), -1)
assert.equal(Model.compareVersions("2.0.0", "1.99.99"), 1)
assert.equal(Model.compareVersions("1.1.0-dev", "1.1.0"), -1)
assert.equal(Model.compareVersions("1.1.0", "1.1.0-dev"), 1)
assert.equal(Model.compareVersions("1.1.0-dev.2", "1.1.0-dev.10"), -1)
assert.equal(Model.compareVersions("1.1.0-alpha", "1.1.0-beta"), -1)
assert.equal(Model.compareVersions("1.1.0-1", "1.1.0-alpha"), -1)
assert.equal(Model.compareVersions("1.0", "1.0.0"), null, "an unreadable version is unknown, not equal")
assert.equal(Model.compareVersions("", "1.0.0"), null)

// The floor is what a source-only `omarchy plugin update` has to survive: the
// checkout moves ahead of the helper, and a helper that still speaks the same
// commands must keep working instead of stopping the plugin dead.
assert.equal(Model.helperSatisfies("1.0.6", "1.0.6"), true)
assert.equal(Model.helperSatisfies("1.0.6", "1.0.7"), true, "a helper past the floor is still usable")
assert.equal(Model.helperSatisfies("1.0.6", "1.0.5"), false)
assert.equal(Model.helperSatisfies("1.1.0", "1.1.0-dev"), false, "a prerelease is below the release")
assert.equal(Model.helperSatisfies("1.0.6", ""), false, "no version reported is not a version that passes")
assert.equal(Model.helperSatisfies("", "1.0.6"), false, "no floor known is not a floor that passes")

// Behind the shipped version but above the floor: offer the update, do not
// stop for it.
assert.equal(Model.helperUpdateAvailable("1.1.0", "1.0.7"), true)
assert.equal(Model.helperUpdateAvailable("1.1.0", "1.1.0"), false)
assert.equal(Model.helperUpdateAvailable("1.1.0", "1.2.0"), false)
assert.equal(Model.helperUpdateAvailable("1.1.0", ""), false)
assert.equal(Model.helperUpdateAvailable("1.1.1-dev", "1.1.0"), false,
  "a development checkout has no matching release asset to offer")
assert.equal(Model.helperUpdateAvailable("1.1.1-rc.1", "1.1.0"), false,
  "prereleases must not offer an optional stable-helper download")

assert.equal(Model.manifestMinHelperVersion('{"id":"oma.nearby","version":"1.1.0","minHelperVersion":"1.0.6"}', "oma.nearby"), "1.0.6")
assert.equal(Model.manifestMinHelperVersion('{"id":"oma.nearby","version":"1.1.0"}', "oma.nearby"), "",
  "a manifest without the field declares no floor, and the service falls back to its own version")
assert.equal(Model.manifestMinHelperVersion('{"id":"other.plugin","minHelperVersion":"1.0.6"}', "oma.nearby"), "")
assert.equal(Model.manifestMinHelperVersion("not json", "oma.nearby"), "")
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

// Quattro accepts a widget id directly in a bar layout and normalizes it to an
// object entry. Reading shell.json directly must preserve those same semantics.
const layoutString = {version:1, bar:{layout:{right:["oma.nearby"]}}, plugins:[]}
assert.deepEqual(Model.barEntry(layoutString, "oma.nearby"), {id:"oma.nearby", settings:{}},
  "a matching string-form bar entry must be recognized as configured")
assert.equal(Model.receiverEnabledIn(Model.barEntry(layoutString, "oma.nearby")), true,
  "a string-form entry with no receiver setting must use the default-on behavior")
for (const region of ["left", "center", "right"]) {
  const config = {version:1, bar:{layout:{left:[], center:[], right:[]}}, plugins:[]}
  config.bar.layout[region] = ["oma.nearby"]
  assert.deepEqual(Model.barEntry(config, "oma.nearby"), {id:"oma.nearby", settings:{}},
    `a string-form entry in bar.layout.${region} must be recognized`)
}
const promotedLayout = {
  version:1,
  bar:{layout:{left:[], center:[], right:["other.before", "oma.nearby", {id:"other.after", x:1}]}},
  plugins:[],
}
assert.equal(Model.hasStringBarEntry(promotedLayout, "oma.nearby"), true)
assert.equal(Model.promoteStringBarEntry(promotedLayout, "oma.nearby", {receiverEnabled:false}), true)
assert.deepEqual(promotedLayout.bar.layout.right,
  ["other.before", {id:"oma.nearby", receiverEnabled:false}, {id:"other.after", x:1}],
  "promoting a string entry must preserve its region, slot, and neighboring entries")
assert.equal(Model.hasStringBarEntry(promotedLayout, "oma.nearby"), false)
assert.equal(Model.promoteStringBarEntry(promotedLayout, "oma.nearby", {receiverEnabled:true}), false,
  "promotion must not append or duplicate an entry that is already object-form")

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
// Malformed entries must not throw or match. String entries are valid only in
// the bar layout; Quattro's top-level plugins[] lookup requires object entries.
assert.equal(Model.barEntry({version:1, bar:{layout:{right:[null,"other.plugin",42]}}}, "oma.nearby"), null)
assert.equal(Model.barEntry({version:1, plugins:["oma.nearby"]}, "oma.nearby"), null)

console.log("Model tests passed")
