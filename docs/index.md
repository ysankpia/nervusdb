# Documentation index

Every document in this repository, and **when to read it**. The "read it when" column
is the useful part — most of these are not worth opening on a first visit, and knowing
which one answers your question is the whole point of an index.

## Using the database

| Document                                | Read it when                                                                                                       |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| [README.md](../README.md)               | First. Install, quick start, capabilities, limitations.                                                            |
| [FORMAT.md](../FORMAT.md)               | You need the exact bytes — writing a reader, or reasoning about durability. The frozen-format contract lives here. |
| [docs/architecture.md](architecture.md) | You want to know _why_ it works: paging, WAL, adjacency, batch weave, locks.                                       |
| [docs/benchmarks.md](benchmarks.md)     | You need a performance number **and the conditions it was measured under**.                                        |

## Working on it

| Document                              | Read it when                                                                                       |
| ------------------------------------- | -------------------------------------------------------------------------------------------------- |
| [AGENTS.md](../AGENTS.md)             | **Before changing code.** Invariants, contracts, verification workflow.                            |
| [docs/testing.md](testing.md)         | Running or adding tests — the CI gate, the red-line totals, the suite inventory, and the adversarial style it follows. |
| [docs/cypher.md](cypher.md)           | Changing the query language — grammar and the executor constraints it depends on.                  |
| [docs/releasing.md](releasing.md)     | Cutting a release, or changing the release workflow.                                               |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Before opening anything. **Issues only; external PRs are closed automatically.**                   |
| [CLA.md](../CLA.md)                   | You have been asked to sign a CLA. The project does not collect them; see CONTRIBUTING.md for why. |

## Status, policy, licensing

| Document                        | Read it when                                                                                         |
| ------------------------------- | ---------------------------------------------------------------------------------------------------- |
| [ROADMAP.md](../ROADMAP.md)     | Deciding whether this fits your use case — done, planned, and explicitly out of scope.               |
| [CHANGELOG.md](../CHANGELOG.md) | Before upgrading. Storage-format and behavioural changes are called out separately. Starts at 0.1.0. |
| [SECURITY.md](../SECURITY.md)   | Reporting a vulnerability, or checking the threat model and known limits.                            |
| [LICENSING.md](../LICENSING.md) | Choosing between AGPL-3.0 and the commercial licence.                                                |

## Agent tooling

Read by the engineering skills, not by people — listed so the file set is not a
mystery when one of them mentions `/triage` or an ADR.

| Document                                                | Contents                                                           |
| ------------------------------------------------------- | ------------------------------------------------------------------ |
| [docs/agents/issue-tracker.md](agents/issue-tracker.md) | Where issues live and how to operate on them.                      |
| [docs/agents/triage-labels.md](agents/triage-labels.md) | The five triage roles, and the label each maps to.                 |
| [docs/agents/domain.md](agents/domain.md)               | How the skills read `CONTEXT.md` / ADRs, and the vocabulary traps. |

`CONTEXT.md` and `docs/adr/` do not exist yet, on purpose: `/domain-modeling` creates
them lazily when a term or decision is actually resolved. Scaffolding them empty would
just be two files nobody reads.

## Historical

| Document                                                        | Note                                                                                                                                              |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| [docs/history/PLAN-1.0.md](history/PLAN-1.0.md)                 | The plan that produced 1.0. Superseded, kept because it records the reasoning.                                                                    |
| [docs/history/CHANGELOG-legacy.md](history/CHANGELOG-legacy.md) | Release history under the previous name (GraphLite, through `v1.0.0`). That line wrote the `GLDB` magic, which this build refuses to reinterpret. |

## Not listed, on purpose

- `LICENSE` — the AGPL-3.0 text, verbatim and unedited.
- Source comments, `--help` output, and generated artifacts — the environment is the
  source of truth for these, and a copy here would go stale.
