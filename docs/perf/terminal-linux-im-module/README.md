# Linux: `GTK_IM_MODULE=wayland` set by Atlas vs the default: page input check (#786)

Private headless sway, private fcitx5 (keyboard-us-intl, keyboard-br, pinyin). `GTK_IM_MODULE` absent from the launch env in A, B, C, F (checked in `/proc/<pid>/environ`: `results/*.launch-env.txt`); D and E set it. Typed through a virtual keyboard with a real xkb keymap (`tools/vk.c`, us-intl and br-abnt2; dead keys at keysym level): `a` `dead_acute c` `dead_tilde a` `o` space, then `é` `ç` `ã` `ê` `à`, then a dead-acute + space (a plain `'`) and `x`. Cell = what the field holds afterwards. `mode`: `layout` = fcitx5 deactivated; `fcitx-kbd` = fcitx5's keyboard (non-pinyin) input method active.

**Result: A, B, C, E and F are identical in all 17 cells** (4 fields x 2 layouts x 2 modes + the pinyin cell). Only explicit `fcitx` (D) differs, in one cell per layout: CodeMirror auto-closes a quote typed through a dead key under the `fcitx` module (`'x'`) and not under the others (`'x`). A user who exports `fcitx` keeps it (Atlas never touches a set value). Nothing regresses in the page relative to the default.

Limitation: in `layout` mode the keys reach GTK without fcitx, and my virtual keyboard's dead keys produce the same (partly garbled for abnt2: `;` `[` `{` from a keymap mismatch) text in every column including A, so that mode does not discriminate; `fcitx-kbd` mode, where fcitx5 composes the accents, is the faithful one (`ação é ç ã ê à ' x` on br-abnt2 in every field). The us-intl results show GTK/fcitx composing `ć` for dead-acute+c in every column and `~c"ao` in fcitx-kbd mode: also identical across columns, so neither is caused by the module.


Row `(d/e) terminal` is an xterm.js terminal in A, C, D, E (native terminal off) and the native GPU terminal in B and F (native terminal on); what it read back is what the shell received (`read -r` into a file).

## intl / layout  (expected `ação é ç ã ê à 'x`)

| field | A | B | C | D | E | F |
|---|---|---|---|---|---|---|
| (a) CodeMirror editor | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x'` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` |
| (b) agent chat input | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` |
| (c) search box | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` |
| (d/e) terminal | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` | `aćão é ć ã ê à 'x` |

## intl / fcitx-kbd  (expected `ação é ç ã ê à 'x`)

| field | A | B | C | D | E | F |
|---|---|---|---|---|---|---|
| (a) CodeMirror editor | `a~c"ao ẽ ~c "a ë 'a ~x'"` | `a~c"ao ẽ ~c "a ë 'a ~x'"` | `a~c"ao ẽ ~c "a ë 'a ~x'"` | `a~c"ao ẽ ~c "a ë 'a ~x'"` | `a~c"ao ẽ ~c "a ë 'a ~x'"` | `a~c"ao ẽ ~c "a ë 'a ~x'"` |
| (b) agent chat input | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` |
| (c) search box | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` |
| (d/e) terminal | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` | `a~c"ao ẽ ~c "a ë 'a ~x` |

## abnt2 / layout  (expected `ação é ç ã ê à ' x`)

| field | A | B | C | D | E | F |
|---|---|---|---|---|---|---|
| (a) CodeMirror editor | `a;áo [e ; á ë {a `x}]` | `a;áo [e ; á ë {a `x}]` | `a;áo [e ; á ë {a `x}]` | `a;áo [e ; á ë {a `x`}]` | `a;áo [e ; á ë {a `x}]` | `a;áo [e ; á ë {a `x}]` |
| (b) agent chat input | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` |
| (c) search box | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` |
| (d/e) terminal | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` | `a;áo [e ; á ë {a `x` |

## abnt2 / fcitx-kbd  (expected `ação é ç ã ê à ' x`)

| field | A | B | C | D | E | F |
|---|---|---|---|---|---|---|
| (a) CodeMirror editor | `ação é ç ã ê à ' x'` | `ação é ç ã ê à ' x'` | `ação é ç ã ê à ' x'` | `ação é ç ã ê à ' x'` | `ação é ç ã ê à ' x'` | `ação é ç ã ê à ' x'` |
| (b) agent chat input | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` |
| (c) search box | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` |
| (d/e) terminal | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` | `ação é ç ã ê à ' x` |

## pinyin preedit in the editor

| A | B | C | D | E | F |
|---|---|---|---|---|---|
| `你好` | `你好` | `你好` | `你好` | `你好` | `你好` |


## Columns

- **A**: GTK_IM_MODULE unset, Atlas leaves it (today)
- **B**: unset, Atlas sets `wayland` (native terminal on)
- **C**: unset, Atlas sets `wayland` (native terminal off)
- **D**: explicit `fcitx`
- **E**: explicit `wayland`
- **F**: unset, native terminal on, Atlas leaves it (throwaway build)

## The reason for the default (native terminal)

With `GTK_IM_MODULE` truly unset and the native terminal on (F), typing `n i h a o` into the native terminal's input sink produced plain `input` events only (`input:n`, `input:i`, ...): no composition, no preedit drawn (`shots/noset-preedit.png`). With Atlas setting `wayland` (B): `compositionstart`, `compositionupdate` `n`, `ni`, `ni h`, `ni ha`, `ni hao`, `compositionend` `你好`, preedit drawn (`shots/atlas-set-native-preedit.png`). Column F's `app.log` line says `set by Atlas` because that throwaway build computed the decision but skipped the `set_var` (`LIM_NOSET`).


## Files
- `results/*.jsonl`: one record per field x layout x mode (`got` vs `expected`); `results-noset/`: column F.
- `shots/`: screenshots (`atlas-set-wayland-gpu1-*`: column B for every cell; `fcitx-gpu1-*`: D with the native terminal on; pinyin preedit/commit).
- `tools/`: the rig (`rig.sh`, `matrix.sh`, `vk.c` virtual keyboard with a real xkb keymap, sway/fcitx5 helpers). They point at `~/.cache/lim-ime` and `~/.cache/LinuxImModule`.

