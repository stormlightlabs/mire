+++
id = "arcl-c-01M6081ZRXN766K7VGBHF21BC9"
title = "Screenshots drop colors that tmux does not repeat"
status = "captured"
created-at = "2026-10-28T03:14:59.997Z"
+++

`.github/storm/shot.sh` renders `tmux capture-pane -e` output with freeze 0.2.2. tmux emits only SGR changes and trims trailing blank cells; freeze drops the foreground when a later SGR sets only an attribute (bold, italic) and paints no background past each line's last character. So bold or italic text with an inherited color shows pale grey, and painted backgrounds stop short, though the terminal draws them. Capturing with `-N` and rewriting each SGR to the full state fixes the PNGs. shot.sh is fabrial's file, so the fix belongs upstream.
