Require cargo
Require git

Set Shell "bash"
Set Width 1600
Set Height 900
Set FontSize 20
Set FontFamily "Iosevka Term"
Set Padding 0
Set Margin 0
Set CursorBlink false
Set TypingSpeed 20ms
Set Theme { "name": "Eldritch", "black": "#171928", "red": "#f16c75", "green": "#37f499", "yellow": "#f1fc79", "blue": "#7081d0", "magenta": "#a48cf2", "cyan": "#04d1f9", "white": "#ebfafa", "brightBlack": "#292e42", "brightRed": "#f16c75", "brightGreen": "#37f499", "brightYellow": "#f7c67f", "brightBlue": "#abb4da", "brightMagenta": "#f265b5", "brightCyan": "#04d1f9", "brightWhite": "#ebfafa", "background": "#171928", "foreground": "#ebfafa", "selection": "#292e42", "cursor": "#f1fc79" }

Hide
Type "cargo build -p mire && rm -f /tmp/mire-screencap-range.json && mire review init /tmp/mire-screencap-range.json HEAD..bf30007"
Enter
Wait+Screen /revision: 1/
Show
Type "unset NO_COLOR; TERM=xterm-256color COLORTERM=truecolor CLICOLOR_FORCE=1 ./target/debug/mire review /tmp/mire-screencap-range.json"
Enter
Wait+Screen /Mire review/
Type "b"
Type "2"
Type "/"
Type "sidebar_visible"
Enter
Wait+Screen /sidebar_visible/
Type "v"
Type "j"
Type "j"
Type "c"
Wait+Screen /New note/
Type "The sidebar state should follow the resolved layout."
Sleep 500ms
Screenshot docs/static/screencap_range.png
Sleep 1s
Escape
Type "q"
Hide
Type "rm -f /tmp/mire-screencap-range.json"
Enter
