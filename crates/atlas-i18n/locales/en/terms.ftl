# Shared words that change with the operating system. Every message may select on $os (mac, windows,
# linux): the i18n layer adds it to every call, so no call site passes it. Reference these from
# other messages as { term-your-computer }; a message reference keeps the caller's arguments, $os
# included. Portuguese needs the preposition to agree with the noun (no Finder, na bandeja), so a message
# that needs "in" or "to" before one of these has its own $os select instead of composing a term.
# Never write "Mac", "Finder", "Explorer" or a shortcut inside a message instead.

# The user's machine, as a possessive noun phrase mid-sentence: "Atlas runs on { term-your-computer }".
term-your-computer = { $os ->
    [mac] your Mac
    [windows] this PC
   *[linux] this computer
}

# The OS app that shows files and folders, mid-sentence: "Reveal in { term-file-manager }".
term-file-manager = { $os ->
    [mac] Finder
    [windows] File Explorer
   *[linux] the file manager
}

# Where deleted files go: "Moved to { term-trash }".
term-trash = { $os ->
    [mac] Trash
    [windows] Recycle Bin
   *[linux] Trash
}

# The OS settings app: "Open { term-system-settings }".
term-system-settings = { $os ->
    [mac] System Settings
    [windows] Settings
   *[linux] system settings
}

# Name of the Alt key as printed on the keyboard, in prose ("hold Option"). Shortcuts use a placeholder, not this.
term-alt-key = { $os ->
    [mac] Option
   *[other] Alt
}

# Name of the confirm key as printed on the keyboard, in prose ("press Return"). Shortcuts use a placeholder, not this.
term-enter-key = { $os ->
    [mac] Return
   *[other] Enter
}

# Where the app's icon lives while it runs in the background: "Atlas stays in { term-menu-bar }".
term-menu-bar = { $os ->
    [mac] the menu bar
   *[other] the system tray
}
