function parseLine(line) {
  try { return JSON.parse(String(line || "")) } catch (e) { return null }
}

function upsertDevice(devices, device) {
  if (!device || !device.fingerprint || !device.alias) return devices || []
  var next = (devices || []).slice()
  var found = -1
  for (var i = 0; i < next.length; i++) if (next[i].fingerprint === device.fingerprint) { found = i; break }
  var row = {
    alias: String(device.alias), version: String(device.version || "2.1"), deviceModel: String(device.deviceModel || ""),
    deviceType: String(device.deviceType || "desktop"), fingerprint: String(device.fingerprint),
    port: Number(device.port || 53317), protocol: String(device.protocol || "https"),
    download: device.download === true, ip: String(device.ip || "")
  }
  if (found < 0) next.push(row); else next[found] = row
  next.sort(function(a, b) { return a.alias.localeCompare(b.alias) })
  return next
}

function snapshotDevices(snapshot) {
  var next = []
  for (var i = 0; i < (snapshot || []).length; i++) next = upsertDevice(next, snapshot[i])
  return next
}

function iconFor(type) {
  if (type === "mobile") return "󰄜"
  if (type === "web") return "󰖟"
  if (type === "server" || type === "headless") return "󰒋"
  return "󰍹"
}

function formatBytes(bytes) {
  var value = Number(bytes || 0)
  if (value < 1024) return value + " B"
  var units = ["KB", "MB", "GB", "TB"], i = -1
  do { value /= 1024; i++ } while (value >= 1024 && i < units.length - 1)
  return value.toFixed(value >= 10 ? 0 : 1) + " " + units[i]
}

function incomingSummary(files) {
  if (!files || files.length === 0) return "Transfer"
  if (files.length === 1) return String(files[0].name || "Transfer")
  return String(files[0].name || "Transfer") + " + " + (files.length - 1) + " more"
}

function enqueueIncoming(queue, request) {
  var next = (queue || []).slice()
  if (!request || !request.requestId) return next
  for (var i = 0; i < next.length; i++) {
    if (next[i] && next[i].requestId === request.requestId) {
      next[i] = request
      return next
    }
  }
  next.push(request)
  return next
}

function removeIncoming(queue, requestId) {
  return (queue || []).filter(function(request) {
    return request && request.requestId !== requestId
  })
}

function currentIncoming(queue) {
  return queue && queue.length ? queue[0] : null
}

function outgoingCommand(pending, transferId, pin) {
  if (!pending || !pending.device || (pending.kind !== "files" && pending.kind !== "text")) return null
  var command = {command:pending.kind === "files" ? "send_files" : "send_text", transfer_id:transferId, device:pending.device}
  if (pending.kind === "files") command.paths = (pending.paths || []).slice()
  else command.text = String(pending.text || "")
  if (typeof pin === "string" && pin !== "") command.pin = pin
  return command
}

function viewAfterOutgoing(hasPendingIncoming, terminalState) {
  return hasPendingIncoming ? "incoming" : terminalState
}

// The plugin's own entry in shell.json, or null when it is not there yet.
//
// The service reads its settings from the shell config rather than waiting for
// a bar widget to hand them over. Widgets are built once per monitor and get
// their `settings` injected a tick after they are created, so a widget cannot
// report the persisted state at the moment it starts up, and several widgets
// reporting the same state is redundant rather than authoritative.
//
// Null means "shell.json has not been applied yet", not "no settings": the
// shell only keeps this plugin loaded while the entry exists, so an absent
// entry is a config that has not arrived rather than one that says nothing.
function barEntry(config, pluginId) {
  var id = String(pluginId || "")
  if (!config || typeof config !== "object" || id === "") return null
  var layout = config.bar && typeof config.bar === "object" ? config.bar.layout : null
  var regions = ["left", "center", "right"]
  for (var r = 0; r < regions.length; r++) {
    var entries = layout && Array.isArray(layout[regions[r]]) ? layout[regions[r]] : []
    for (var e = 0; e < entries.length; e++) {
      var entry = entries[e]
      if (typeof entry === "string") {
        if (entry === id) return { id: id, settings: {} }
        continue
      }
      if (!entry || typeof entry !== "object") continue
      if (String(entry.id || "") !== id) continue
      var settings = {}
      for (var key in entry) if (key !== "id") settings[key] = entry[key]
      return { id: String(entry.id), settings: settings }
    }
  }
  var plugins = Array.isArray(config.plugins) ? config.plugins : []
  for (var p = 0; p < plugins.length; p++) {
    var plugin = plugins[p]
    if (!plugin || typeof plugin !== "object" || String(plugin.id || "") !== id) continue
    var pluginSettings = {}
    for (var pluginKey in plugin) if (pluginKey !== "id") pluginSettings[pluginKey] = plugin[pluginKey]
    return { id: String(plugin.id), settings: pluginSettings }
  }
  return null
}

// Quattro accepts a bare id in bar.layout, but its inline-settings writer can
// only attach settings to object entries. Promote a matching string in place
// before writing the first setting; top-level plugins[] does not accept this
// form in the host and is deliberately excluded.
function hasStringBarEntry(config, pluginId) {
  var id = String(pluginId || "")
  var layout = config && config.bar && typeof config.bar === "object" ? config.bar.layout : null
  if (!layout || id === "") return false
  var regions = ["left", "center", "right"]
  for (var r = 0; r < regions.length; r++) {
    var entries = Array.isArray(layout[regions[r]]) ? layout[regions[r]] : []
    for (var e = 0; e < entries.length; e++) if (entries[e] === id) return true
  }
  return false
}

function promoteStringBarEntry(config, pluginId, settings) {
  var id = String(pluginId || "")
  var layout = config && config.bar && typeof config.bar === "object" ? config.bar.layout : null
  if (!layout || id === "") return false
  var regions = ["left", "center", "right"]
  for (var r = 0; r < regions.length; r++) {
    var entries = Array.isArray(layout[regions[r]]) ? layout[regions[r]] : []
    for (var e = 0; e < entries.length; e++) {
      if (entries[e] !== id) continue
      var promoted = { id: id }
      for (var key in settings) if (key !== "id") promoted[key] = settings[key]
      entries[e] = promoted
      return true
    }
  }
  return false
}

// Receiving is on unless the entry says otherwise, and off until the entry
// exists at all, so a persisted "off" cannot be missed while the config is
// still loading.
function receiverEnabledIn(entry) {
  return !!entry && entry.settings.receiverEnabled !== false
}

function helperVersionMatches(pluginVersion, helperVersion) {
  return String(pluginVersion || "") !== "" && String(pluginVersion) === String(helperVersion || "")
}

// Nearby's release process only ever produces MAJOR.MINOR.PATCH with an
// optional prerelease suffix, so that is all this reads. Anything else is
// null, which callers treat as unknown rather than as equal: a version that
// cannot be read is not one the plugin can vouch for.
function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(String(value || "").trim())
  if (!match) return null
  return {
    release: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4] ? match[4].split(".") : []
  }
}

// SemVer precedence: numeric fields compare as numbers, a prerelease sorts
// before the release it leads up to, and prerelease identifiers compare as
// numbers when both are numeric and as text otherwise.
function compareVersions(left, right) {
  const a = parseVersion(left)
  const b = parseVersion(right)
  if (!a || !b) return null
  for (var i = 0; i < 3; i++) {
    if (a.release[i] !== b.release[i]) return a.release[i] < b.release[i] ? -1 : 1
  }
  if (a.prerelease.length === 0 || b.prerelease.length === 0) {
    if (a.prerelease.length === b.prerelease.length) return 0
    return a.prerelease.length === 0 ? 1 : -1
  }
  const longest = Math.max(a.prerelease.length, b.prerelease.length)
  for (var j = 0; j < longest; j++) {
    if (j >= a.prerelease.length) return -1
    if (j >= b.prerelease.length) return 1
    const x = a.prerelease[j]
    const y = b.prerelease[j]
    const xNumeric = /^\d+$/.test(x)
    const yNumeric = /^\d+$/.test(y)
    if (xNumeric && yNumeric) {
      if (Number(x) !== Number(y)) return Number(x) < Number(y) ? -1 : 1
      continue
    }
    if (xNumeric !== yNumeric) return xNumeric ? -1 : 1
    if (x !== y) return x < y ? -1 : 1
  }
  return 0
}

// A helper is good enough when it is at least the oldest one this plugin knows
// how to drive. Requiring the exact shipped version instead broke the plugin on
// every release: `omarchy plugin update` moves the source and cannot move the
// binary, because bin/ is not tracked, so a plugin one version ahead of a
// helper it could still talk to refused to run at all. An unreadable version on
// either side is still a refusal.
function helperSatisfies(requiredVersion, helperVersion) {
  const order = compareVersions(helperVersion, requiredVersion)
  return order !== null && order >= 0
}

// Usable but behind: worth offering an update, not worth stopping for.
function helperUpdateAvailable(pluginVersion, helperVersion) {
  // Release assets exist only for stable versions. A compatible helper may be
  // older than a development checkout, but offering an update in that state
  // can only lead to the updater's "no published helper" failure.
  const plugin = parseVersion(pluginVersion)
  if (!plugin || plugin.prerelease.length !== 0) return false
  const order = compareVersions(helperVersion, pluginVersion)
  return order !== null && order < 0
}

function manifestVersion(text, pluginId) {
  try {
    const manifest = JSON.parse(String(text || ""))
    if (String(manifest.id || "") !== String(pluginId || "")) return ""
    return String(manifest.version || "")
  } catch (_) {
    return ""
  }
}

// The oldest helper this plugin can drive, declared beside the version it
// ships with. Absent means "the shipped version", which is the rule the plugin
// followed before the field existed and the safe reading of a manifest that
// predates it.
function manifestMinHelperVersion(text, pluginId) {
  try {
    const manifest = JSON.parse(String(text || ""))
    if (String(manifest.id || "") !== String(pluginId || "")) return ""
    return String(manifest.minHelperVersion || "")
  } catch (_) {
    return ""
  }
}

if (typeof module !== "undefined") module.exports = { parseLine, upsertDevice, snapshotDevices, iconFor, formatBytes, incomingSummary, enqueueIncoming, removeIncoming, currentIncoming, outgoingCommand, viewAfterOutgoing, barEntry, hasStringBarEntry, promoteStringBarEntry, receiverEnabledIn, helperVersionMatches, compareVersions, helperSatisfies, helperUpdateAvailable, manifestVersion, manifestMinHelperVersion }
