Require cargo

Set Shell "bash"
Set Width 1600
Set Height 900
Set FontSize 20
Set Padding 0
Set Margin 0
Set CursorBlink false
Set TypingSpeed 20ms
Set Theme { "name": "Eldritch", "black": "#171928", "red": "#f16c75", "green": "#37f499", "yellow": "#f1fc79", "blue": "#7081d0", "magenta": "#a48cf2", "cyan": "#04d1f9", "white": "#ebfafa", "brightBlack": "#292e42", "brightRed": "#f16c75", "brightGreen": "#37f499", "brightYellow": "#f7c67f", "brightBlue": "#abb4da", "brightMagenta": "#f265b5", "brightCyan": "#04d1f9", "brightWhite": "#ebfafa", "background": "#171928", "foreground": "#ebfafa", "selection": "#292e42", "cursor": "#f1fc79" }

Hide
Type "cargo build --quiet -p mire && git show --format= 3b53b72^! -- crates/ui/src/app.rs crates/ui/src/chrome.rs crates/ui/src/view.rs > /tmp/mire-screencap.patch"
Enter
Wait
Show
Type "unset NO_COLOR; TERM=xterm-256color COLORTERM=truecolor CLICOLOR_FORCE=1 ./target/debug/mire --theme eldritch patch /tmp/mire-screencap.patch"
Enter
Wait+Screen /Mire review/
Type "/"
Sleep 100ms
Type "render_note_editor"
Enter
Wait+Screen /render_note_editor/
Sleep 200ms
Screenshot .github/assets/screencap.png
Sleep 1s
Type "q"
Hide
Type "rm -f /tmp/mire-screencap.patch"
Enter
