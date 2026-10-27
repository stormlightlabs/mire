+++
id = "arcl-c-01M5Z0F0GYCQ3V379RASCK0R51"
title = "Teach the bundled review skill to mark files viewed"
status = "captured"
created-at = "2026-10-27T15:43:03.710Z"
+++

The bundled agent skill (crates/cli/skills/mire/SKILL.md) does not mention `mire file viewed`, `mire file unviewed` or `mire context --reader`. Agents share `USER` with the person, so the skill should tell them to always pass `--reader` to keep their marks apart.
