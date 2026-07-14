# MIRE

Model Independent Review Environment

```mermaid
flowchart LR
    CLI["CLI commands and terminal renderer"] --> Core["Application core"]
    Browser["SvelteKit app"] --> HTTP["Local HTTP API"]
    HTTP --> Core
    Core --> Git["Local Git"]
    Core --> Store["Review session store"]
    Core --> Models["Model adapters"]
    HTTP --> Assets["Embedded SvelteKit assets"]
```

## References

1. <https://google.github.io/eng-practices/review/reviewer/looking-for.html>
2. <https://developers.googleblog.com/conductor-update-introducing-automated-reviews/>
3. <https://www.greptile.com/what-is-ai-code-review>
