+++
id = "arcl-c-01M7P2DNCKK035FWFK41ZFFHFD"
title = "Markdown export breaks code spans on backticks in paths"
status = "captured"
created-at = "2026-11-18T00:55:30.451Z"
+++

The Markdown notes export wraps paths and other inline values in single backticks. A path or producer name that holds a backtick ends the code span early, so the rest of the line renders as Markdown. Control characters are already escaped; this is only about backticks. Fix by sizing the code-span delimiter to the longest backtick run, as the body fence does.
