# Generated from the clap tree by `UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n`.
# Do not edit by hand: change the doc comment in the Rust source and regenerate.

# Short description of `atlas config` in `--help`.
cli-config-about = Validate atlas.yaml, print its JSON Schema or set the display language

# Short description of `atlas config check` in `--help`.
cli-config-check-about = Validate atlas.yaml (+ atlas.local.yaml); prints OK or every error, exit 1 on errors

# Short description of `atlas config schema` in `--help`.
cli-config-schema-about = Print the JSON Schema for atlas.yaml to stdout

# Short description of `atlas config lang` in `--help`.
cli-config-lang-about = Show or set the display language (en, pt-BR); `ATLAS_LANG` overrides it per command

# Help line of the `tag` argument of `atlas config lang`.
cli-config-lang-arg-tag = Language to save in ~/.atlas/language (en, pt-BR); omit to show the current one

# Help line of the `unset` argument of `atlas config lang`.
cli-config-lang-arg-unset = Remove the saved language: follow the system language

# Help line of the `json` argument of `atlas config lang`.
cli-config-lang-arg-json = Print JSON (language, source, preference, supported); machine output is never translated

