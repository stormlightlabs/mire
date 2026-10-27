+++
id = "arcl-t-01M5YRTHTMYF1H67PS6X7ZVG8F"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Teach the bundled mire skill the walkthrough commands"
status = "pending"
priority = "normal"
position = 0
+++

The bundled agent skill (crates/cli/skills/mire/SKILL.md) covers notes but not `mire walkthrough apply|show|clear`, and its example batch still names protocol minor 1. Add a short walkthrough section so agents find the commands without the CLI reference.

Verification:
- The skill names `mire walkthrough apply`, `show` and `clear` with one example, and its example batch uses the current protocol minor.
- `just check` passes.
