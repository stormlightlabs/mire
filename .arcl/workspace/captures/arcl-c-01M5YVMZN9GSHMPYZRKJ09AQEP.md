+++
id = "arcl-c-01M5YVMZN9GSHMPYZRKJ09AQEP"
title = "Store review byte strings compactly"
status = "captured"
created-at = "2026-10-27T14:18:56.553Z"
+++

The review file stores `ByteString` and `BytePath` as JSON number arrays (`#[serde(transparent)]` over `Vec<u8>` in `crates/core/src/model.rs`). That makes a review file about 5 times the size of its patch: a 92,308-byte patch gave a 457,106-byte review. So the 64 MiB patch limit (`DEFAULT_MAX_PATCH_BYTES`) does not keep a review under the 128 MiB write limit (`DEFAULT_MAX_REVIEW_FILE_BYTES`). A compact encoding needs a schema change and a reader for the old form.
