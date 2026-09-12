# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to
the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the
corresponding label string from this table.

## State of these labels in the repo

Checked against `gh label list` on 2026-09-13:

- `wontfix` **already exists** (GitHub's default, description "This will not be
  worked on"). No action needed.
- The other four do not exist yet and the `triage` skill will create them on first use.

If the project later prefers different strings — for example a `bug:` / `type:`
namespace to match an existing scheme — edit the right-hand column here so `triage`
applies the existing labels instead of creating a second set that means the same thing.
