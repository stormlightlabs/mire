+++
id = "arcl-t-01M5SXDG6FZY355BG5BNHPMG4R"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "TypeScript state and diff tests"
status = "pending"
priority = "normal"
position = 1
+++

Add vitest coverage for the app's review state, diff rendering data, filters, and revision-conflict handling in `packages/app`.

Verification:
- `pnpm --filter @stormlightlabs/mire-app test:unit --run` covers them, and `just check` passes.
