# MIRE

Model Independent Review Environment

```mermaid
flowchart LR
    CLI["CLI commands and terminal renderer"] --> Core["Review service"]

    Assets["Embedded SvelteKit assets"] --> Server["Loopback Go server"]
    Server -->|"Serves app"| Browser["SvelteKit app"]
    Browser -->|"JSON + SSE"| Server
    Server --> Core

    Core --> Git["Read-only Git and snapshot capture"]
    Core --> Store["SQLite and private object store"]
    Core --> Models["Model adapters"]
    Core --> Analyzers["Optional Setaryb and Mccabre CLI adapters"]
```

## References

1. <https://google.github.io/eng-practices/review/reviewer/looking-for.html>
2. <https://developers.googleblog.com/conductor-update-introducing-automated-reviews/>
3. <https://www.greptile.com/what-is-ai-code-review>
