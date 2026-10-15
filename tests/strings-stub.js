const english = require('../i18n/en.json')

function format(text, args) {
  if (!args || args.length === 0) return text
  return String(text).replace(/%(\d+)/g, (match, index) => {
    const value = args[Number(index) - 1]
    return value === undefined ? match : String(value)
  })
}

function entry(key) {
  if (!Object.hasOwn(english, key)) throw new Error(`Unknown Nearby string: ${key}`)
  return english[key]
}

module.exports = {
  t(key, args) { const value = entry(key); return format(typeof value === 'string' ? value : value.other, args) },
  tn(key, count, args) { const value = entry(key); return format(typeof value === 'string' ? value : value[count === 1 ? 'one' : 'other'], args) },
  number(value, decimals) { return decimals === undefined ? String(value) : Number(value).toFixed(decimals) },
  rightToLeft: false,
}
