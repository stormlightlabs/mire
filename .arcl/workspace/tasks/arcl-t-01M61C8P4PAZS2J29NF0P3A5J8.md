+++
id = "arcl-t-01M61C8P4PAZS2J29NF0P3A5J8"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Quieter finding badges and a distinct inline finding line"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #24 at 6c89a33: finding lines read as annotations with a severity rule in the terminal and browser; severity marks are a dot and word; severity colours pass 4.5:1 in truecolor and 256 colours."
+++

In the diff, the inline finding line blends into the surrounding code and the severity badges are heavy. Restyle both as part of the reading-layout work, in the terminal and the browser.

Acceptance criteria:
- [ ] The finding line reads as separate from the code around it in every palette.
- [ ] Severity badges are lighter while staying distinguishable by severity.
- [ ] The rendered-frame contrast test passes for the new styles.
- [ ] Shots and browser screenshots, checked by eye.
