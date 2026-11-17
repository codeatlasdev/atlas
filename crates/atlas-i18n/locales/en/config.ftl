# First line of `atlas config lang` with no argument. $lang is a language tag such as pt-BR.
config-lang-current = Display language: { $lang }

# Second line of `atlas config lang`: which input decided the language. $source is one of the
# keys below. The environment variable and file names are literal and stay as they are.
config-lang-source = { $source ->
    [env] Source: the ATLAS_LANG environment variable
    [file] Source: the saved preference (~/.atlas/language)
    [posix] Source: the LC_ALL, LC_MESSAGES or LANG environment variable
    [os] Source: the system language
   *[default] Source: none set, using English
}

# Third line of `atlas config lang`: the preference saved in ~/.atlas/language. $lang is a tag.
config-lang-saved = Saved preference: { $lang }

# Third line of `atlas config lang` when no preference is saved.
config-lang-saved-none = Saved preference: none, Atlas follows the system language

# Last line of `atlas config lang`. $count is how many languages exist, $langs the list of tags.
config-lang-available = { $count ->
    [one] Available language: { $langs }
   *[other] Available languages: { $langs }
}

# Confirmation after `atlas config lang <tag>`. $lang is the saved tag.
config-lang-set = Display language set to { $lang }.

# Warning after `atlas config lang <tag>` when ATLAS_LANG is set: the saved language is not in
# effect in this shell. $value is the value of ATLAS_LANG.
config-lang-set-env-warning = ATLAS_LANG={ $value } is set in this shell and takes precedence over the saved preference.

# Confirmation after `atlas config lang --unset`.
config-lang-unset = Saved preference removed. Atlas follows the system language.

# Error when `atlas config lang <tag>` gets a language that has no catalog. $tag is what the
# user typed, $langs the list of valid tags.
config-lang-unsupported = Unsupported language "{ $tag }". Available: { $langs }
