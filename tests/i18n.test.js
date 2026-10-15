const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const vm = require('node:vm')
const Model = require('../Model.js')
const strings = require('./strings-stub.js')
const root = path.resolve(__dirname, '..')
const languages = require('./i18n-languages.json')
const REQUIRE_COMPLETE = true // Every language supported by Omarchy needs a complete catalogue.

// JSON.parse discards duplicate keys. Walk the tokens first, tracking each
// object's own keys, including escaped spellings and nested plural objects.
function parseUnique(raw) {
  const result = JSON.parse(raw)
  const tokens = raw.match(/"(?:[^"\\]|\\.)*"|[{}\[\]:,]|true|false|null|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g) || []
  let at = 0
  function value() {
    const token = tokens[at++]
    if (token === '{') {
      const seen = new Set()
      while (tokens[at] !== '}') {
        const key = JSON.parse(tokens[at++])
        assert.ok(!seen.has(key), `Duplicate JSON key: ${key}`)
        seen.add(key)
        assert.equal(tokens[at++], ':')
        value()
        if (tokens[at] !== ',') break
        at++
      }
      assert.equal(tokens[at++], '}')
    } else if (token === '[') {
      while (tokens[at] !== ']') { value(); if (tokens[at] !== ',') break; at++ }
      assert.equal(tokens[at++], ']')
    }
  }
  value()
  assert.equal(at, tokens.length)
  return result
}
assert.throws(() => parseUnique('{"a":1,"\\u0061":2}'), /Duplicate/)
assert.throws(() => parseUnique('{"a":{"one":"x","one":"y"}}'), /Duplicate/)
assert.deepEqual(parseUnique('{"a":"one","b":{"one":"two"}}'), {a:'one',b:{one:'two'}})

const english = parseUnique(fs.readFileSync(path.join(root, 'i18n/en.json'), 'utf8'))
const notes = parseUnique(fs.readFileSync(path.join(root, 'docs/translation-notes.json'), 'utf8'))
assert.deepEqual(Object.keys(notes).sort(), Object.keys(english).sort(), 'Every key needs translation context')
for (const note of Object.values(notes)) assert.ok(typeof note === 'string' && note.trim())
const placeholders = text => (text.match(/%[1-9]\d*/g) || []).sort()
const protectedWords = ['Nearby', 'LocalSend', 'GitHub', 'PIN', 'XDG', 'settings.json', 'MiB', 'wl-copy', 'wl-paste', '·', '…']
function checkText(text, source, key, code) {
  const label = `${code}: ${key}`
  assert.ok(typeof text === 'string' && text.trim().length > 0, `${label}: empty/non-string translation`)
  assert.deepEqual(placeholders(text), placeholders(source), `${label}: placeholders changed`)
  assert.equal(text.trim(), text, `${label}: leading/trailing whitespace`)
  for (const word of protectedWords) if (source.includes(word)) assert.ok(text.includes(word), `${label}: preserve ${word}`)
  if (key.startsWith('section.')) assert.equal(text, text.toLocaleUpperCase(code), `${label}: uppercase heading`)
}
function validate(catalogue, code, complete) {
  assert.ok(catalogue && typeof catalogue === 'object' && !Array.isArray(catalogue))
  const missing = Object.keys(english).filter(key => !Object.hasOwn(catalogue, key))
  if (complete) {
    assert.deepEqual(missing, [], `${code}: missing keys`)
    assert.deepEqual(Object.keys(catalogue), Object.keys(english), `${code}: preserve source key order`)
  }
  for (const [key, value] of Object.entries(catalogue)) {
    assert.ok(Object.hasOwn(english, key), `${code}: unknown key ${key}`)
    const source = english[key]
    if (typeof source === 'string') checkText(value, source, key, code)
    else {
      assert.ok(value && typeof value === 'object' && !Array.isArray(value), `${code}: ${key}: plural required`)
      assert.deepEqual(Object.keys(value).sort(), [...languages[code].plural].sort(), `${code}: ${key}: plural categories`)
      for (const text of Object.values(value)) checkText(text, source.other, key, code)
    }
  }
  return missing.length
}
assert.throws(() => validate({...english, 'action.done':'Done %1'}, 'en', true), /placeholders/)
assert.throws(() => validate({...english, 'bar.ready':'Ready'}, 'en', true), /preserve/)
assert.throws(() => validate({...english, 'incoming.moreRequests':{other:'%1 requests'}}, 'en', true), /plural categories/)
assert.throws(() => validate({...english, 'section.send':'Send'}, 'en', true), /uppercase/)
assert.throws(() => validate({...english, 'action.done':''}, 'en', true), /empty/)
const used = new Set()
for (const file of ['Panel.qml', 'Service.qml', 'Model.js']) {
  const source = fs.readFileSync(path.join(root, file), 'utf8')
  // Includes conditional keys and the rotating phrase array, as well as t/tn.
  for (const match of source.matchAll(/"((?:bar|hero|phrase|receiver|section|devices|action|folder|chooser|send|clipboard|helper|pin|incoming|transfer|status|notify|engine)\.[A-Za-z]+)"/g)) used.add(match[1])
}
assert.deepEqual([...used].sort(), Object.keys(english).sort(), 'Used keys and English catalogue must match exactly')

const omarchy = process.env.OMARCHY_PATH || '/usr/share/omarchy'
const registry = path.join(omarchy, 'default/i18n/locales.json')
const pluralPath = path.join(omarchy, 'shell/Commons/Plural.js')
if (fs.existsSync(registry) && fs.existsSync(pluralPath)) {
  const actual = JSON.parse(fs.readFileSync(registry, 'utf8'))
  const plural = require(pluralPath)
  assert.deepEqual(Object.keys(languages).sort(), Object.keys(actual).sort(), 'Regenerate language snapshot from installed Omarchy')
  for (const [code, entry] of Object.entries(actual)) {
    assert.deepEqual(languages[code], {name:entry.name,englishName:entry.englishName,direction:entry.direction || 'ltr',plural:plural.categories(code)}, `${code}: regenerate language snapshot`)
  }
}
let count = 0
const files = fs.readdirSync(path.join(root, 'i18n'))
for (const required of ['en.json','es-MX.json']) assert.ok(files.includes(required), `Missing required catalogue: ${required}`)
for (const file of files) {
  assert.ok(file.endsWith('.json'), `Unexpected catalogue file: ${file}`)
  const code = file.slice(0, -5)
  assert.ok(Object.hasOwn(languages, code), `Unsupported Omarchy language: ${code}`)
  const missing = validate(parseUnique(fs.readFileSync(path.join(root, 'i18n', file), 'utf8')), code, REQUIRE_COMPLETE || code === 'en' || code === 'es-MX')
  if (missing) console.warn(`${code}: ${missing} untranslated keys; English fallback applies`)
  count++
}
if (REQUIRE_COMPLETE) assert.deepEqual(files.sort(), Object.keys(languages).map(code => `${code}.json`).sort())

// Evaluate the formatter shipped in QML, not just its test stub.
const qml = fs.readFileSync(path.join(root, 'Strings.qml'), 'utf8')
const start = qml.indexOf('  function format(')
const end = qml.indexOf('\n  function englishValue(', start)
const format = vm.runInNewContext(`(${qml.slice(start, end).trim()})`)
for (const formatter of [format, (text,args) => strings.t('incoming.wantsToSend', args)]) {
  assert.equal(formatter('%1 wants to send', ['%2 evil']), '%2 evil wants to send')
}
assert.equal(format('%1 %2 %1 %10', ['%2','B',3,4,5,6,7,8,9,'J']), '%2 B %2 J')
assert.equal(format('%1 %2', ['A']), 'A %2')
assert.equal(Model.incomingSummary([{name:'%2 evil'},{name:'b'}],strings), '%2 evil + 1 more')
assert.equal(Model.incomingSummary([],strings),'Transfer')
assert.equal(Model.formatBytes(1536,strings),'1.5 KB')
const comma = {...strings, number:(n,d) => strings.number(n,d).replace('.',',')}
assert.equal(Model.formatBytes(1536,comma),'1,5 KB')
assert.equal(Model.formatBytes(1536),'1.5 KB')
console.log(`i18n tests passed (${count}/${Object.keys(languages).length} supported catalogues, ${Object.keys(english).length} keys)`)
