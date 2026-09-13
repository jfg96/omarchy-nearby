const assert = require("node:assert/strict")
const Model = require("../Model.js")

const phone = {alias:"<b>Phone</b>",fingerprint:"fp",deviceType:"mobile",ip:"192.0.2.2"}
assert.equal(Model.upsertDevice([], phone).length, 1)
assert.equal(Model.upsertDevice(Model.upsertDevice([], phone), {...phone, alias:"Phone 2"})[0].alias, "Phone 2")
assert.deepEqual(Model.snapshotDevices([phone, {...phone, alias:"Newest"}]).map(d => d.alias), ["Newest"])

// The frontend device array is the Rust peer registry's mirror and must never
// exceed the same bound: LAN identity churn has to stay capped even while the
// discovery view is closed. One constant drives both event and snapshot paths.
const MAX_DEVICES = Model.MAX_DEVICES
assert.equal(typeof MAX_DEVICES, "number")
assert.ok(MAX_DEVICES > 0)

function deviceAt(fingerprint, alias) {
  return {fingerprint, alias, deviceType:"desktop", ip:"192.0.2.2"}
}

// Normal behavior with few devices is unchanged.
{
  let list = []
  for (let i = 0; i < 5; i++) list = Model.upsertDevice(list, deviceAt("fp-"+i, "Alias "+i))
  assert.equal(list.length, 5)
}

// More than MAX_DEVICES unique fingerprints never grow the array past the bound.
{
  let list = []
  const extra = 100
  for (let i = 0; i < MAX_DEVICES + extra; i++) list = Model.upsertDevice(list, deviceAt("fp-"+i, "Device-"+String(i).padStart(4, "0")))
  assert.equal(list.length, MAX_DEVICES)
}

// Updating an existing fingerprint does not increase the size at the bound.
{
  let list = []
  for (let i = 0; i < MAX_DEVICES; i++) list = Model.upsertDevice(list, deviceAt("fp-"+i, "Device-"+String(i).padStart(4, "0")))
  const before = list.length
  list = Model.upsertDevice(list, deviceAt("fp-0", "Renamed"))
  assert.equal(list.length, before)
  const renamed = list.find(device => device.fingerprint === "fp-0")
  assert.equal(renamed.alias, "Renamed")
}

// A snapshot of more than MAX_DEVICES devices returns at most MAX_DEVICES.
{
  const many = []
  for (let i = 0; i < MAX_DEVICES + 50; i++) many.push(deviceAt("snap-"+i, "Snap-"+String(i).padStart(4, "0")))
  const snapshot = Model.snapshotDevices(many)
  assert.ok(snapshot.length <= MAX_DEVICES)
}

// Deduplication by fingerprint keeps working, inside and beyond the bound.
assert.deepEqual(Model.snapshotDevices([deviceAt("x", "A")]).length, 1)
assert.deepEqual(Model.snapshotDevices([deviceAt("x", "A"), deviceAt("x", "B")]).map(device => device.alias), ["B"])
{
  let list = []
  const branded = {}
  for (let i = 0; i < MAX_DEVICES + 20; i++) {
    branded["brand-"+i] = "Branded "+i
    list = Model.upsertDevice(list, deviceAt("brand-"+i, "Branded "+i))
  }
  list = Model.upsertDevice(list, deviceAt("brand-0", "Renamed Brand"))
  assert.equal(list.length, MAX_DEVICES)
  assert.equal(list.filter(device => device.fingerprint === "brand-0").length, 1)
}

// Alias ordering stays deterministic at the bound.
{
  let list = []
  for (let i = 0; i < MAX_DEVICES + 10; i++) list = Model.upsertDevice(list, deviceAt("fp-"+i, "Device-"+String(i).padStart(4, "0")))
  for (let i = 1; i < list.length; i++) assert.ok(list[i-1].alias.localeCompare(list[i].alias) <= 0)
}

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

// The compatibility config parser backs the modern-host file fallback: an
// object is the config, anything else is a config that has not arrived.
assert.deepEqual(Model.parseShellConfig('{"bar":{"layout":{}}}'), {bar:{layout:{}}})
assert.deepEqual(Model.parseShellConfig("{}"), {})
assert.equal(Model.parseShellConfig("not json"), null)
assert.equal(Model.parseShellConfig(""), null)
assert.equal(Model.parseShellConfig("null"), null)
assert.equal(Model.parseShellConfig('["oma.nearby"]'), null,
  "a JSON array is not a shell config and must stay null")

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
