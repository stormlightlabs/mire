# Mire implementation work

Source: [ROADMAP.md](ROADMAP.md)

## Milestone 6: Broader interoperability

Exit criterion: Mire works as a pager, difftool, direct-file reviewer, or
supported VCS client and can exchange findings through SARIF without weakening
the core review rules.

### M6.1 Add pager, difftool, and direct-file modes

- [ ] Redirected output, noninteractive input, and quit behavior match the
      selected mode.
- [ ] Difftool mode accepts one file pair without assuming a repository-wide
      patch.
- [ ] Direct-file mode watches both input paths and preserves TUI state across
      reloads.

Verification:

```text
cargo test -p mire --test integration_modes
```

Smoke-test pager and difftool invocation in a disposable fixture repository.

### M6.2 Add Jujutsu and Sapling adapters

- [ ] Pass native revision selectors and path filters as separate subprocess
      arguments.
- [ ] Support explicit VCS selection and deterministic repository detection.
- [ ] Missing or unsupported tools fail clearly without silently falling back
      to Git.
- [ ] Shared patch fixtures produce changesets equivalent to Git where the
      compared content is the same.

Verification:

```text
cargo test -p mire --test vcs_adapters
```

Smoke-test each adapter where its native tool is available.

### M6.3 Export SARIF and publish through optional forge adapters

- [ ] SARIF validates against its schema and preserves locations, severity,
      rule identity, and provenance.
- [ ] Unsupported review fields produce visible warnings instead of silent
      data loss.
- [ ] Network publication is opt-in, previewable, idempotent, and isolated from
      `mire-core` and `mire-tui`.

Verification:

```text
cargo test -p mire --test sarif
```

Validate exported fixtures with an independent SARIF validator.

## Milestone 7: Review expressiveness and quality

After Milestone 6:

- [ ] Add optional confidence, evidence, and structured remediation for
      suggestions.
- [ ] Add related locations for findings that span multiple code sites.
- [ ] Derive reviewer-quality summaries from provenance and dispositions.

## Later candidates

- additional themes, large configuration systems, embedded providers, MCP,
  provider-specific adapters, and structural diffing.
