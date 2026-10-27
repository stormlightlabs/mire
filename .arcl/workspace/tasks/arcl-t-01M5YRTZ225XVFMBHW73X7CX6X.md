+++
id = "arcl-t-01M5YRTZ225XVFMBHW73X7CX6X"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Mark generated files from .gitattributes"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
+++

Read `linguist-generated` (and `-diff`) from the reviewed repository's `.gitattributes` when building the changeset. Mark those files in the API and the terminal file list, and collapse them in the browser by default. Mire's own rules can come later if `.gitattributes` is not enough.

Verification:
- A terminal snapshot test shows a generated file marked in the file list.
- The Playwright review flow covers the new states, and `just check` passes.
