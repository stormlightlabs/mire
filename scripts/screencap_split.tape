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
Set Theme { "name": "Catppuccin Mocha", "black": "#45475a", "red": "#f38ba8", "green": "#a6e3a1", "yellow": "#f9e2af", "blue": "#89b4fa", "magenta": "#f5c2e7", "cyan": "#94e2d5", "white": "#bac2de", "brightBlack": "#585b70", "brightRed": "#f38ba8", "brightGreen": "#a6e3a1", "brightYellow": "#f9e2af", "brightBlue": "#89b4fa", "brightMagenta": "#f5c2e7", "brightCyan": "#94e2d5", "brightWhite": "#a6adc8", "background": "#1e1e2e", "foreground": "#cdd6f4", "selection": "#585b70", "cursor": "#f5e0dc" }

Type "unset NO_COLOR; TERM=xterm-256color COLORTERM=truecolor CLICOLOR_FORCE=1 mire diff 751692a..bf30007 --theme catppuccin"
Enter
Wait+Screen /Mire review/
Type "b"
Type "2"
Wait+Screen /split/
Sleep 500ms
Screenshot docs/static/screencap_split.png
Sleep 1s
Type "q"
