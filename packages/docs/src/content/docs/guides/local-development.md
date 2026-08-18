---
title: Local development
description: Run the Mire API server and browser frontend together for UI development.
section: Guides
group: Guides
order: 10
---

The browser review surface has two parts: the Rust API server (`mire serve`) and
the Svelte frontend (`packages/app`). During development, Vite proxies API
requests from the frontend to the Rust server.

## Prerequisites

- Rust 1.88 or newer
- Node.js 22 or newer
- pnpm 11 or newer

## Start the servers

Open two terminals from the repository root.

**Terminal 1 — Rust API server:**

```sh
cargo run -p mire -- serve examples/de47985-847ffe1.json --port 3737
```

The server prints a session URL:

```
Mire review server: http://127.0.0.1:3737/#<session-secret>
```

**Terminal 2 — Vite dev server:**

```sh
pnpm --filter @stormlightlabs/mire-app run dev
```

This starts the frontend on `http://localhost:5173`.

## Open the review

Open `http://localhost:5173`, then paste the complete session URL printed by
`mire serve` into the connection form. You can paste the secret after `#`
instead if you prefer.

The development app keeps the secret for the current browser tab, so page
reloads and Vite updates reconnect without another paste. When you restart
`mire serve`, paste the newly printed URL into the form.

## How the proxy works

The Vite config in `packages/app/vite.config.ts` proxies `/api` requests to the
Rust server:

```ts
const mireServerOrigin = process.env.MIRE_SERVER_ORIGIN ?? 'http://127.0.0.1:3737';

server: {
  proxy: {
    '/api': {
      target: mireServerOrigin,
      changeOrigin: true
    }
  }
}
```

If you change the `--port` on `mire serve`, set `MIRE_SERVER_ORIGIN` to match:

```sh
MIRE_SERVER_ORIGIN=http://127.0.0.1:4000 pnpm --filter @stormlightlabs/mire-app run dev
```

## Build the frontend

When you are ready to test a production build:

```sh
pnpm --filter @stormlightlabs/mire-app run build
```

This writes the static site to `crates/cli/assets/web`, which the Rust binary
embeds with `include_dir`. The next `cargo build` or `cargo run` picks up the
new assets.

To preview the production build locally:

```sh
pnpm --filter @stormlightlabs/mire-app run preview
```

This serves the built assets on `http://localhost:4173`. The preview server has
no API proxy — it only works when embedded in the `mire serve` binary.

## Run tests

```sh
# Unit tests
pnpm --filter @stormlightlabs/mire-app run test:unit

# Type checking
pnpm --filter @stormlightlabs/mire-app run check

# Full suite (unit + e2e)
pnpm --filter @stormlightlabs/mire-app run test
```

Playwright e2e tests require a running `mire serve` instance because the app
needs a session secret to render review data.

## Troubleshooting

### "This review URL is missing its session secret"

Paste the full session URL from the `mire serve` output into the connection
form. The form also accepts the secret after `#` by itself.

### Port already in use

If port 3737 is occupied, pass a different port to `mire serve`:

```sh
cargo run -p mire -- serve review.json --port 4000
```

Then set the matching origin for Vite:

```sh
MIRE_SERVER_ORIGIN=http://127.0.0.1:4000 pnpm --filter @stormlightlabs/mire-app run dev
```

If port 5173 is occupied, Vite picks the next available port automatically.
Check the terminal output for the actual URL.

### API requests return 401

The session secret is per-process. If you restart `mire serve`, the old secret
is invalid. Paste the new URL from the terminal into the connection form.

### "Review unavailable" after editing the review file

If another process (like `mire review refresh`) modifies the review file, the
server detects the revision change and returns a conflict. Click **Reload
review** in the browser, or refresh the page.

### Frontend changes not appearing

Vite hot-reloads frontend changes automatically. If changes are not reflected:

1. Check the Vite terminal for compilation errors.
2. Restart the Vite dev server.

### CSS or font changes not appearing

The embedded fonts and styles are served from `packages/app/src/app.css`. After
changes, Vite hot-reloads them in dev mode. For production builds, run
`pnpm --filter @stormlightlabs/mire-app run build` to update the embedded
assets.
