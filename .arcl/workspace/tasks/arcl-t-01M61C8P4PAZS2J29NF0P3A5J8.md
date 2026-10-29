+++
id = "arcl-t-01M61C8P4PAZS2J29NF0P3A5J8"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Quieter finding badges and a distinct inline finding line"
status = "in_progress"
priority = "normal"
position = 0
+++

In the diff, the inline finding line blends into the surrounding code and the severity badges are heavy. Restyle both as part of the reading-layout work, in the terminal and the browser.

Acceptance criteria:
- [ ] The finding line reads as separate from the code around it in every palette.
- [ ] Severity badges are lighter while staying distinguishable by severity.
- [ ] The rendered-frame contrast test passes for the new styles.
- [ ] Shots and browser screenshots, checked by eye.
