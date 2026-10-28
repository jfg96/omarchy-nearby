const assert = require("node:assert/strict")
const fs = require("node:fs")
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
assert.equal(Model.hasIncoming([requestA, requestB], "b"), true)
assert.equal(Model.hasIncoming([requestA, requestB], "missing"), false)
assert.equal(Model.hasIncoming([requestA], ""), false)
assert.equal(Model.hasIncoming(null, "a"), false)

// Clicking an incoming-transfer notification opens the panel, but only for a
// request that is still pending. A shell whose notification helper predates
// --exec refuses the option without showing anything, so the plain
// notification is sent instead.
{
  const notifyArgs = ["--app-name", "Nearby", "--urgency", "normal", "Incoming transfer", "From Alice: a.txt"]
  const command = Model.incomingNotificationCommand("req-1", notifyArgs)
  assert.equal(command[0], "bash")
  assert.equal(command[1], "-c")
  assert.deepEqual(command.slice(3), ["nearby-notify", "req-1", ...notifyArgs],
    "the request id and every notification argument must reach the script as separate arguments")
  assert.deepEqual(Model.incomingNotificationCommand("", notifyArgs), ["omarchy-notification-send", ...notifyArgs],
    "a request without an id has nothing to reopen and keeps the plain notification")

  const { spawnSync } = require("node:child_process")
  const os = require("node:os")
  const path = require("node:path")
  const stubs = fs.mkdtempSync(path.join(os.tmpdir(), "nearby-notify-"))
  const log = path.join(stubs, "calls")
  fs.writeFileSync(path.join(stubs, "omarchy-notification-send"),
    "#!/bin/bash\nprintf '%s\\n' \"$(printf '%s\\x1f' \"$@\")\" >> \"$NOTIFY_LOG\"\n" +
    "if [[ -n ${NOTIFY_LEGACY:-} ]]; then for a in \"$@\"; do [[ $a == --exec ]] && { echo 'Unknown option: --exec' >&2; exit 1; }; done; fi\nexit 0\n",
    {mode: 0o755})
  const run = (requestId, args, legacy) => {
    fs.rmSync(log, {force: true})
    const env = {PATH: `${stubs}:/usr/bin:/bin`, NOTIFY_LOG: log}
    if (legacy) env.NOTIFY_LEGACY = "1"
    const [program, ...rest] = Model.incomingNotificationCommand(requestId, args)
    const result = spawnSync(program, rest, {env, encoding: "utf8"})
    assert.equal(result.status, 0, result.stderr)
    return fs.readFileSync(log, "utf8").trim().split("\n").map(line => line.split("\x1f").slice(0, -1))
  }

  assert.deepEqual(run("req-1", notifyArgs, false),
    [[...notifyArgs, "--exec", "omarchy-shell", "-q", "oma.nearby", "showRequest", "req-1"]],
    "a current helper gets one notification whose click asks the service to show that request")
  assert.deepEqual(run("req-1", notifyArgs, true),
    [[...notifyArgs, "--exec", "omarchy-shell", "-q", "oma.nearby", "showRequest", "req-1"], notifyArgs],
    "a helper without --exec must still deliver the notification, without the click action")

  const hostile = ["--app-name", "Nearby", "--urgency", "normal", "Incoming transfer", "From $(touch pwned) `id` \"; exit 0"]
  assert.deepEqual(run("$(touch pwned)", hostile, false),
    [[...hostile, "--exec", "omarchy-shell", "-q", "oma.nearby", "showRequest", "$(touch pwned)"]],
    "arguments must stay data and never be re-parsed by the shell")
  assert.equal(fs.existsSync(path.join(stubs, "pwned")) || fs.existsSync("pwned"), false)
  fs.rmSync(stubs, {recursive: true, force: true})
}
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

// The folder for received files is read from the entry. Only an absolute path
// is a choice: the helper ignores anything else, so showing it would name a
// folder that is not used.
assert.equal(Model.downloadDirIn({downloadDir: "/home/someone/Pictures/Phone"}), "/home/someone/Pictures/Phone")
for (const value of ["", "Pictures", "~/Pictures", "./in", null, undefined, 42, ["/tmp"], {path: "/tmp"}]) {
  assert.equal(Model.downloadDirIn({downloadDir: value}), "", `${JSON.stringify(value)} is not a folder choice`)
}
assert.equal(Model.downloadDirIn(null), "")
assert.equal(Model.downloadDirIn({}), "")
// The home folder and the folders above it are not a destination: a sender
// names the files and folders, which would land among the user's own.
for (const path of ["/home/someone", "/home/someone/", "/home", "/", "//home//someone", "/home/someone/Pictures/..", "/home/./someone"]) {
  assert.equal(Model.containsHome(path, "/home/someone"), true, `${path} holds the home folder`)
}
for (const path of ["/home/someone/Downloads", "/home/someone/Pictures/Phone", "/home/someone2", "/home/other", "/data/in", "/run/media/someone/Stick"]) {
  assert.equal(Model.containsHome(path, "/home/someone"), false, `${path} does not hold the home folder`)
}
assert.equal(Model.containsHome("/", ""), true, "without a known home folder the root is still refused")
assert.equal(Model.containsHome("/data/in", ""), false)
assert.equal(Model.folderName("/home/someone/Downloads"), "Downloads")
assert.equal(Model.folderName("/home/someone/Downloads/"), "Downloads")
assert.equal(Model.folderName("/"), "/")
assert.equal(Model.folderName(""), "/")
assert.equal(Model.folderName("/data/" + "x".repeat(40)), "x".repeat(23) + "…",
  "a long folder name must be cut: the kit's buttons do not elide")
// The preview decodes a peer's file, so only common raster types are let
// through, and only by the type read from the bytes. Only PNG, JPEG, WebP and
// GIF are decoded inside the shell. Everything else is converted to a PNG by
// ImageMagick in a separate process first: phone formats a stock shell cannot
// decode, and TIFF and BMP, whose decoders the shell must not run on a peer's
// file, since a fault there would take the whole desktop shell down with it.
for (const type of ["image/png", "image/jpeg", "image/webp", "image/gif"]) {
  assert.equal(Model.previewableImage(type), true, `${type} must be previewable`)
  assert.equal(Model.imagePreviewMode(type), "direct", `${type} is decoded by the shell`)
}
for (const type of ["image/heic", "image/heif", "image/avif", "image/jxl", "image/tiff", "image/bmp", "image/x-ms-bmp"]) {
  assert.equal(Model.previewableImage(type), true, `${type} must be previewable`)
  assert.equal(Model.imagePreviewMode(type), "convert", `${type} must never be decoded inside the shell`)
}
for (const type of ["image/svg+xml", "image/x-xcf", "image/heic-sequence", "application/pdf", "text/plain", "IMAGE/PNG", "", null, undefined]) {
  assert.equal(Model.previewableImage(type), false, `${type} must not reach an image decoder`)
  assert.equal(Model.imagePreviewMode(type), "")
}

// The scripts that decode a received picture name their decoder by type. Each
// must cover exactly the types the panel treats as pictures, so a picture is
// never previewed but copied as a file, or converted without a preview.
{
  const scriptTypes = name => {
    const text = fs.readFileSync(require.resolve(`../bin/${name}`), "utf8")
    const block = text.slice(text.indexOf('case "$(file --brief --mime-type'), text.indexOf("esac"))
    return [...block.matchAll(/^\s*([a-z0-9/.+|-]+)\)/gm)].flatMap(match => match[1].split("|")).sort()
  }
  const all = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp", "image/x-ms-bmp", "image/tiff",
    "image/heic", "image/heif", "image/avif", "image/jxl"]
  assert.deepEqual(scriptTypes("nearby-copy-file"), [...all].sort(),
    "every previewable picture must also be copied as a picture")
  assert.deepEqual(scriptTypes("nearby-preview-image"), all.filter(type => Model.imagePreviewMode(type) === "convert").sort(),
    "the preview converter must decode exactly the types the shell cannot")
}
assert.equal(Model.fileUrl("/home/someone/Downloads/photo.jpg"), "file:///home/someone/Downloads/photo.jpg")
assert.equal(Model.fileUrl("/tmp/a b/é#?%.png"), "file:///tmp/a%20b/%C3%A9%23%3F%25.png",
  "a received name must not be read as a URL query or fragment")

// Speed and time left come from the progress events of the last few seconds.
// A short or empty window shows nothing rather than a wild first guess, and
// bytes that go backwards, as when the receiver rolls back a failed upload,
// start the window again.
{
  const strings = require("./strings-stub.js")
  const MiB = 1024 * 1024
  let samples = []
  samples = Model.progressSamples(samples, 0, 0)
  samples = Model.progressSamples(samples, 1000, 10 * MiB)
  assert.equal(Model.progressRate(samples), 0, "one second of data is too little to show a speed")
  assert.equal(Model.progressDetail(samples, 100 * MiB, strings), "")
  samples = Model.progressSamples(samples, 2000, 20 * MiB)
  assert.equal(Model.progressRate(samples), 10 * MiB)
  assert.equal(Model.progressDetail(samples, 140 * MiB, strings), "10 MB/s · 15 s left",
    "120 MiB left at 10 MiB/s is 12 seconds, shown in steps of five")

  // Only the last five seconds count, so the speed follows a change.
  for (let t = 3000; t <= 8000; t += 1000) samples = Model.progressSamples(samples, t, 20 * MiB + (t - 2000) * 1024)
  assert.ok(samples[0].time >= 3000, "samples older than the window are dropped")
  assert.equal(Model.progressRate(samples), 1024 * 1000)

  const reset = Model.progressSamples(samples, 9000, 5 * MiB)
  assert.deepEqual(reset, [{time: 9000, bytes: 5 * MiB}], "bytes going backwards restart the window")
  assert.equal(Model.progressDetail(reset, 100 * MiB, strings), "")
  assert.equal(Model.progressDetail([], 0, strings), "")

  const stalled = [{time: 0, bytes: 5 * MiB}, {time: 3000, bytes: 5 * MiB}]
  assert.equal(Model.progressDetail(stalled, 100 * MiB, strings), "", "no movement has no speed to show")

  // Durations are rounded so the line does not flicker: seconds in steps of
  // five, whole minutes, then hours and minutes.
  assert.equal(Model.formatDuration(3, strings), "a few seconds")
  assert.equal(Model.formatDuration(12, strings), "15 s")
  assert.equal(Model.formatDuration(59, strings), "1 min")
  assert.equal(Model.formatDuration(61, strings), "2 min")
  assert.equal(Model.formatDuration(3599, strings), "60 min")
  assert.equal(Model.formatDuration(3600, strings), "1 h 0 min")
  assert.equal(Model.formatDuration(5 * 3600 + 30 * 60, strings), "5 h 30 min")
}

console.log("Model tests passed")
