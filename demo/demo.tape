# Regenerate (from the repo root, after `cargo build -p atlas-cli`):
#   vhs demo/demo.tape   -> site/public/media/demo.gif (README, served from the site; not in git) + demo/.out/tui.mp4 (1600x1000 take for the film)
#
# Shows the current `atlas dev` log UX on the demo stack (demo/atlas.yaml):
# boot order, restart separator, find with n/N counter, ]e error jumps,
# link focus (URL + path:line), JSON compaction, level filter, time range,
# confirm action that pauses a service.
Output site/public/media/demo.gif
Output demo/.out/tui.mp4

Set Shell "bash"
Set FontFamily "JetBrains Mono"
Set FontSize 17
Set Width 1600
Set Height 1000
Set Padding 22
Set Framerate 30
Set PlaybackSpeed 1.0
Set TypingSpeed 70ms
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set CursorBlink false
Env COLORTERM "truecolor"

Hide
Type "unset NO_COLOR && cd demo && rm -rf .logs && export PS1='\[\e[35m\]orbit-shop\[\e[0m\] ❯ ' && clear"
Enter
Sleep 300ms
Show

Type "atlas dev"
Sleep 400ms
Hide
Ctrl+U
Type "../target/debug/atlas dev"
Enter
Show
# boot: migrate -> api -> web/worker, payments crash-loops and gives up
Sleep 9s

# errors: ]e jumps between error lines, wherever they are
Type "g"
Sleep 600ms
Type "]e"
Sleep 1.5s
Type "]e"
Sleep 1.5s
Type "]e"
Sleep 1.5s

# links: { walks the URLs / path:line refs; o opens, y copies
Type "G"
Sleep 600ms
Type "{"
Sleep 1.3s
Type "{"
Sleep 1.3s
Type "{"
Sleep 1.3s
Type "G"
Sleep 500ms

# find: live counter + highlight, n/N step through hits
Type "/stripe"
Sleep 1.4s
Enter
Sleep 500ms
Type "n"
Sleep 1.3s
Type "N"
Sleep 1.3s
Escape
Sleep 500ms

# api tab: JSON logs compacted into readable lines
Type "2"
Sleep 1.4s
Type "J"
Sleep 2s
Type "0"
Sleep 500ms

# level filter: warn+ then errors only
Type "v"
Sleep 1.6s
Type "v"
Sleep 1.8s
Type "v"
Sleep 600ms

# time range: last 1m, then 5m
Type "V"
Sleep 1.4s
Type "V"
Sleep 1.4s
Type "VVVVVV"
Sleep 800ms
Escape
Sleep 500ms

# command palette -> confirm modal -> action pauses worker
Type ":"
Sleep 600ms
Type@120ms "seed"
Sleep 1s
Enter
Sleep 1.5s
Enter
Sleep 3s

Type "q"
Sleep 800ms
Type "y"
Sleep 1.5s
