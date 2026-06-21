# Doc Gardening

## When To Run

Do a doc-gardening pass after:

- Substantial implementation or refactor.
- Bug fix that reveals stale documentation.
- Architecture change that moves boundaries.
- Validation change that alters the default check path.
- Long handoff (more than 2 weeks between active sessions).
- Any change that affects public behavior, API, storage format, or validation.

## Checklist

### Links

- `docs/index.md` entries point to existing files.
- All cross-references between docs (e.g. "see `docs/engineering/...`") resolve.
- Deleted legacy docs are not referenced from current docs.

### Plans

- Completed plans are moved from `docs/plans/active/` to `docs/plans/completed/`.
- Superseded plans are marked with status "superseded" and reference the
  superseding plan.
- Active plans reflect current work and are not stale.

### Bug Records

- Open bugs have reproduction steps and root cause when known.
- Resolved bugs have regression guards documented.
- No closed bug is reopened without a new reproduction.

### Quality Score

- Recheck `docs/engineering/quality-score.md` evidence.
- Update scores if code or documentation quality has measurably changed.
- Record the recheck date.

### Technical Debt

- Move accepted or deferred cleanup into `docs/plans/tech-debt.md`.
- Retire debt entries that are no longer relevant.

### Architecture Invariants

- Verify that `docs/engineering/architecture-invariants.md` still matches
  current code boundaries.
- Add new invariants discovered during implementation.

### Stale Instructions

- Remove instructions that do not match current code behavior.
- Update command examples that no longer produce the documented output.
- Remove references to deleted or renamed modules.

## Tools

```bash
# Check all doc paths referenced in docs/index.md resolve
for f in $(rg -o 'docs/[^)]+' docs/index.md | sort -u); do
  test -f "$f" || echo "MISSING: $f"
done

# Find stale references to deleted archive paths
rg -n "docs/archive/|legacy-platform-era" docs/ AGENTS.md README.md README_CN.md \
  --glob '!docs/runbooks/doc-gardening.md'

# Check for broken internal links
rg -o '\[.*\]\((/?docs/[^)]+)\)' docs/ | rg -v 'http' | while IFS=: read -r file link; do
  path=$(echo "$link" | sed -n 's/.*(\(.*\))/\1/p')
  test -f "$path" || echo "BROKEN: $file -> $path"
done
```
