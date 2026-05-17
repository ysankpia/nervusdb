# Design T103: Compaction Integration (Index/Properties Safety)

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Depends On**: T102 (IndexCatalog)

## 0. The Real Problem

Current compaction writes a new CSR segment and then emits:

- `ManifestSwitch`
- `Checkpoint { up_to_txid }`

That checkpoint tells recovery to **skip replaying** transactions `<= up_to_txid`.

This is only correct if *all* data from those transactions has been persisted into `.ndb`.

Right now, node/edge properties live in:

- WAL (for recovery), and
- in-memory runs (for reads),

and are **not** persisted into `.ndb` by compaction.

So checkpointing after compaction can silently drop properties on restart.

## 1. T103 MVP Rule

Compaction is allowed to checkpoint only when it is lossless.

MVP criterion:

- If the compacted runs contain **any properties**, compaction MUST NOT emit a `Checkpoint`.
- It may still emit a `ManifestSwitch` (segments are real and durable).
- It must not clear runs, because runs are still the only read path for properties.

## 2. Future (T103+ / T104+)

Once properties are persisted into `.ndb` (either via a dedicated property store or index-backed KV),
compaction can become fully lossless and will be allowed to checkpoint+clear runs again.

## 3. Tests

- Create a node, set a property, run `compact()`, restart engine, ensure property is still readable.
- Ensure edge-only compaction still checkpoints and clears runs (existing behavior).

