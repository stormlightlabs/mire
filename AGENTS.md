# AGENTS Guide

- Use Writing Rust & Reviewing Rust
  - Separate modules for maintainability and responsibilities/concerns
  - Symbol Order: constants, types, enums, structs, functions, then public before private
    within each category
  - Include rustdoc comments for exported & important symbols
  - no `pub(super)` or `pub(crate)`
- Use writing for documentation
  - Don't include references to milestones or tickets (internal planning language)
    ANYWHERE in source code
  - Keep docs/ up to date with usage instructions, again with the writing skill
- The TUI can and should be verified with a herdr pane. Open a background tab
  and send key/mouse events. Use the current working tree as the diff to check.
