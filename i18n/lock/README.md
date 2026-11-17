# Lock files

`i18n/lock/<surface>/<locale>/<area>.json` records, for every key a locale has translated, the hash of the English
text it was translated from. That is how `bun run i18n:check` knows a translation is outdated when the English changes.

```json
{
  "settings_display_language_label": "3fa1c2d09b7e"
}
```

- `surface` is `desktop`, `site` or `rust` (the name of the app directory, or `rust`); `area` is the part of the key
  before its first `_` (Fluent: the `.ftl` file name).
- The value is the first 12 hex chars of the sha256 of the normalized English value: for a Paraglide catalog the
  `JSON.stringify` of the value (a string, or the variants array), for Fluent the normalized message source (value
  and attributes, no comment). The description is never hashed, so improving it does not invalidate translations.
- Sorted keys, one per line, two-space indent, LF, trailing newline.
- **Never edit these by hand.** `bun run i18n:translate` stamps what it writes; `bun run i18n:fix` stamps translations
  that exist and are not outdated, removes stale entries and re-formats. After a typo-only edit of an English text,
  `bun run i18n:fix --restamp <key>` records the new hash without retranslating.
