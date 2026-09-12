# Domain Docs

How the engineering skills should consume this repo's domain documentation when
exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root — the domain glossary.
- **`docs/adr/`** — read ADRs that touch the area you're about to work in.

If either doesn't exist, **proceed silently**. Don't flag their absence; don't suggest
creating them upfront. The `/domain-modeling` skill (reached via `/grill-with-docs` and
`/improve-codebase-architecture`) creates them lazily when terms or decisions actually
get resolved.

## Layout: single-context

This repo is single-context. It is a Cargo workspace, but not a monorepo in the sense
that matters here: `src/` is one library with one domain vocabulary, `bindings/python`
and `bindings/nodejs` are thin FFI surfaces over it, and `benches/` is instrumentation.
There are no independently-evolving bounded contexts with their own language.

```
/
├── CONTEXT.md            ← the glossary (created lazily)
├── docs/adr/             ← decisions (created lazily)
├── src/                  ← the engine: the one context
├── bindings/             ← FFI surfaces over it (no separate vocabulary)
└── benches/              ← acceptance instruments
```

## Where this repo already documents things

Two files here are older and more authoritative than a glossary, and a new term should
agree with them rather than be invented beside them:

- **`FORMAT.md`** — the byte-level format contract. Terms like _page_, _slot_,
  _packed property pointer_, _magic_, and _format version_ are defined here, and this
  file is frozen: changing a byte changes the term's meaning.
- **`AGENTS.md`** — the invariants. Terms like _baseline_, _spill_, _weave_,
  _deferred eviction_ carry specific meanings in the rules there.

`docs/index.md` lists everything and when to read it.

The glossary should therefore _reference_ these rather than restate them. A second
definition of "packed property pointer" is a liability: it will drift, and a reader
cannot tell which one the code follows.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a
hypothesis, a test name), use the term as defined. Don't drift to synonyms the glossary
explicitly avoids.

Two vocabulary traps worth knowing up front, because this codebase has hit both:

- **Nodes and edges share an ID numbering space.** Never write a bare `id`; say which.
  The `Binding` enum exists precisely to keep them apart at the type level.
- **"Latch" and "lock" are not interchangeable here.** The engine has a global
  buffer-pool mutex and _no_ page-level latching — `BufferPoolManager::latch` is dead
  code. Writing "page latch" describes something that does not exist; see AGENTS.md
  §10.

If the concept you need isn't in the glossary yet, that's a signal — either you're
inventing language the project doesn't use (reconsider) or there's a real gap (note it
for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently
overriding:

> _Contradicts ADR-0007 (event-sourced orders) — but worth reopening because…_
