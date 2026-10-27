+++
id = "arcl-c-01M5Z3Q9NX4KYJS8W9BA2ZZHSC"
title = "Measure history size without serializing all of it"
status = "captured"
created-at = "2026-10-27T16:40:00.957Z"
+++

Each refresh that records a checkpoint serializes the whole history twice to measure it: once to compact to the 16 MiB budget and once to keep patch copies within it (crates/core/src/review/checkpoint.rs). Near the budget that is about 32 MiB of JSON per refresh, and the budget test in crates/core/tests/checkpoints.rs takes about 30 s in a debug build. Caching each checkpoint's serialized size, or tracking it as rows and copies change, would make the check proportional to the change.
