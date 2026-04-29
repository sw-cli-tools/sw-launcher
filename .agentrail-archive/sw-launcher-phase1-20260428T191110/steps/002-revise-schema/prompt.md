# Step 2: revise-schema

Take the gaps surfaced by step 1's `docs/survey/schema-gaps.md` and
update `docs/design.md` so every survey repo can be expressed as a
`sw-launch.toml` without loss. No Rust is written here.

## Inputs

- `docs/survey/index.md`
- `docs/survey/<repo>.md` for each repo
- `docs/survey/schema-gaps.md`
- `docs/survey/tuplet-failure-hypothesis.md`

## What to revise

For each gap in `docs/survey/schema-gaps.md`, decide one of:

- add a new TOML field (give it a name, a type, a default, an
  example, and a validation rule with a stable error code)
- generalize an existing field (cite the old form, the new form,
  and why every existing example still parses)
- reject the gap as out of scope (cite the reason; record in
  `docs/design.md` "Out of scope" section)

Do not invent fields whose only justification is "might be useful
later." Every change traces to a survey repo.

## Update targets

- `docs/design.md`:
  - schema sections for `[targets.*]`, `[scenarios.*]`,
    `[layers.*]`, `[layers.*.segments.*]`, `[tools.*]`,
    `[profiles.*]`
  - validation rules (extend the E0001..E00NN list as needed; keep
    existing numbers stable)
  - scenario A/B/C example TOMLs (re-verify they still parse against
    the revised schema)
- `docs/architecture.md`:
  - update "Layers are composites" or "Key invariants" if any new
    invariant was introduced
- `docs/plan.md`:
  - if any phase needs reordering because of a discovered
    dependency, do it now
- `docs/status.md`:
  - record that schema is now grounded in survey

## Pre-commit gate

```bash
markdown-checker -f "docs/**/*.md"
sw-checklist
```

ASCII-only markdown.

## Done when

- Every gap in `docs/survey/schema-gaps.md` has an entry in
  `docs/design.md` (added field, generalized field, or
  out-of-scope decision with rationale).
- The three scenario examples in `docs/design.md` still parse
  conceptually against the revised schema.
- New error codes are appended (never renumbered).
- `docs/status.md` mentions schema revision and date.
- Commit message lists the gaps addressed.
- `agentrail complete --summary "..." --reward 1`.

## Stop after complete

Step 3 (scaffold-cli) is a fresh session.
