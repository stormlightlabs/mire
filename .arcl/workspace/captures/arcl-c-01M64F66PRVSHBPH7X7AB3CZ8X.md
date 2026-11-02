+++
id = "arcl-c-01M64F66PRVSHBPH7X7AB3CZ8X"
title = "Iceberg Dark added and removed rows look alike at 256 colours"
status = "captured"
created-at = "2026-10-29T18:36:35.928Z"
+++

At 256 colours, Iceberg Dark's added rows (`#2e3130`) and removed rows (`#32232b`) in `ICEBERG_DARK_256` (`crates/ui/src/theme.rs`) round to neighbouring greys, 236 and 235. Apart from the + and - marks, the two kinds of row look almost the same. Pick 256-set row colours that round to tinted cube colours, such as Iceberg's own `ctermbg=29` and `ctermbg=95`, while keeping text at 4.5:1 after rounding and every surface distinct.
