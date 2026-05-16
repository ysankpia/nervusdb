# Bug Ledger

Use this directory for durable bug records, root causes, regression guards, and
prevention notes.

## Structure

- `docs/bugs/open/` for active bugs.
- `docs/bugs/resolved/` for fixed bugs with regression guards.
- `docs/bugs/template.md` for new records.

## Rule

Do not close a recurring or shipped bug without a regression guard. If a
deterministic test is not practical, document the alternative guard and why it is
acceptable.
