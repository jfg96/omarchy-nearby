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
  var groups = []
  var layout = config.bar && typeof config.bar === "object" ? config.bar.layout : null
  var regions = ["left", "center", "right"]
  for (var r = 0; r < regions.length; r++) {
    if (layout && Array.isArray(layout[regions[r]])) groups.push(layout[regions[r]])
  }
  if (Array.isArray(config.plugins)) groups.push(config.plugins)
  for (var g = 0; g < groups.length; g++) {
    for (var e = 0; e < groups[g].length; e++) {
      var entry = groups[g][e]
      if (!entry || typeof entry !== "object") continue
      if (String(entry.id || "") !== id) continue
      var settings = {}
      for (var key in entry) if (key !== "id") settings[key] = entry[key]
      return { id: String(entry.id), settings: settings }
    }
  }
  return null
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

function manifestVersion(text, pluginId) {
  try {
    const manifest = JSON.parse(String(text || ""))
    if (String(manifest.id || "") !== String(pluginId || "")) return ""
    return String(manifest.version || "")
  } catch (_) {
    return ""
  }
}

if (typeof module !== "undefined") module.exports = { parseLine, upsertDevice, snapshotDevices, iconFor, formatBytes, incomingSummary, enqueueIncoming, removeIncoming, currentIncoming, outgoingCommand, viewAfterOutgoing, barEntry, receiverEnabledIn, helperVersionMatches, manifestVersion }
