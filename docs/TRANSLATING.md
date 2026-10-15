# Translating Nearby

Nearby follows the display language selected under **Setup > Language** in
Omarchy. Supported language codes come from Omarchy's
`default/i18n/locales.json`; `tests/i18n-languages.json` records the codes,
directions and integer plural categories used by the supported shell. Update
that snapshot when Omarchy changes its registry. Do not substitute OS locale
names: Spanish uses `es-MX`, Norwegian `no`, and Chinese `zh-CN` / `zh-TW`.

## Catalogue format

Add `i18n/<code>.json`, a flat JSON object in the same order as `i18n/en.json`.
Keep blank lines between groups. Every English key needs a translation; consult
`translation-notes.json` for its location and placeholder meanings. English and
Spanish are complete examples. Every language in the supported registry must
have a complete catalogue; CI rejects missing languages or translation keys.

- Values are strings, or plural objects with exactly the categories in the
  language snapshot. Keep placeholders `%1`, `%2`, etc. exactly, including
  their number of occurrences, in every plural form. Reorder them freely.
- Keep Nearby, LocalSend, GitHub, PIN, XDG, settings.json, MiB, B/KB/MB/GB/TB,
  wl-copy and wl-paste unchanged. Commands and paths arrive through placeholders.
  Preserve ` · ` separators and the single-character ellipsis `…`.
- Use short, natural desktop interface wording. Follow the tone and terms in
  Omarchy's `default/i18n/<code>/shell.json` where useful.
- `section.*` values are uppercase in scripts with letter case. Hero subtitles
  are uppercased by the shell: use normal case in their catalogue entries.
- Aim for at most 24 characters in short hero/status labels and 14 in buttons
  shown side by side. These are guides; preserve meaning and inspect layout.
- Do not join translated sentence fragments. Introduce a complete sentence
  with placeholders when adding visible text. Treat filenames and peer names
  as remote input, and escape notification arguments before substitution.

Run `node tests/i18n.test.js` after each catalogue. It rejects duplicate keys,
unsupported language codes, unknown keys, incorrect types, plural categories,
placeholder changes, empty values, reordered keys and altered protected words
(including PIN). With a local
Omarchy installation it also compares the language snapshot against that
installation. Completeness is required for every language and every key.

## Runtime behavior

`Strings.qml` delegates to the shell's `Commons.Translations` when available.
Missing strings fall back through the base language, English, then the key.
Older shells without this type load English directly. Panel bindings follow
language changes immediately. Service messages assigned during an event keep
the language of that event until the next state change. Diagnostic messages
from the Rust helper and repair scripts remain in English; translating them
requires a separate helper protocol change. User documentation stays English.

RTL layout comes from the shell's mirrored panel; Nearby reverses its Back
chevron. Test Arabic, long German/Finnish labels, plural forms, and a language
switch with the panel open. Never claim real-device or older-shell validation
unless those checks were actually performed.

The initial catalogues are prepared with AI assistance; native-language review
and corrections are welcome.
