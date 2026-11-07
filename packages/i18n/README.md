# swofty-i18n

The launcher's translations. The English text in the code is the key, and text with no translation stays in English.

## Add a language

Add `locales/<code>.json`, with the language's own name under `_language`:

```json
{
  "_language": "Deutsch",
  "Settings": "Einstellungen",
  "{name} is the active profile": "{name} ist das aktive Profil"
}
```

It then shows up under **Language** in Settings. You don't have to translate everything: copy `ru.json` for the full list, and leave out what you haven't done yet.

The launcher's layout runs left to right only, so a right-to-left language won't lay out correctly.

## Translate more of the launcher

Wrap the text where the code uses it, then add it to the locale files:

```rust
use swofty_i18n::{tr, trf};

.child(tr("Settings"))
.child(trf!("{name} is the active profile", name = name))
```

`tr` returns a `&'static str`, so it fits wherever a literal did. Keep the text in the call: `cargo test -p swofty-i18n` looks for it there, and fails if a locale file translates text the launcher no longer has or drops a `{name}`.
