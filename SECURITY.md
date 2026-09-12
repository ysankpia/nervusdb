# Security Policy

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Email **luhuizhx@gmail.com** with:

- a description of the issue and its impact;
- the version or commit you tested;
- a reproduction, if you have one (the smaller the better);
- whether you intend to disclose it, and on what timeline.

You will get an acknowledgement, and an honest assessment of whether it is
exploitable and how it will be handled. If a fix is needed, it will be developed
privately and released before any public write-up.

## Scope

GraphLite-RS is an embedded database: it runs **in the caller's process**, not as
a server. That shapes what counts as a vulnerability.

**In scope**

- Memory-safety issues: out-of-bounds reads or writes, use-after-free, panics
  reachable from untrusted input (for example a corrupt or hostile database
  file, or a Cypher string).
- Data-integrity failures: committing or exposing uncommitted data, rollback that
  leaves residue, WAL replay applying a torn or uncommitted frame, recovery that
  silently loses a committed transaction, integrity checks that pass on a
  corrupt database.
- Filesystem-safety issues in the open path: writing outside the database file,
  symlink or path handling that could let a file be overwritten unexpectedly.
- Denial of service from a crafted Cypher query or database file that is cheap to
  produce but hard to survive (unbounded memory, non-terminating traversal).
- Supply chain: a dependency that is compromised or has a known-exploitable flaw.

**Not in scope (by design, not a bug)**

- Any attack that requires the attacker to already write to the database file or
  run code in the same process. An embedded database trusts its own process and
  its own file; if an attacker has either, the game is already lost.
- Loss of a database file when the process or the disk fails between an
  `execute` and its commit. That is why transactions exist.
- Reading the database file to extract data. It is not encrypted; filesystem
  permissions are the access control. There is no encryption feature, and its
  absence is not a vulnerability.

## Known limitations that are documented, not vulnerabilities

These are recorded in [ROADMAP.md](ROADMAP.md) rather than treated as security
bugs. If you believe one of them is worse than documented, that is worth
reporting:

- Readers and a writer cannot hold the database **simultaneously**: read-only handles
  take a shared lock and coexist with each other, but a write handle excludes them and
  vice versa. `GraphLite::read_snapshot()` gives a caller a consistent view **for the
  duration of the snapshot**, which closes the correctness gap, but it does not add
  concurrency — it still blocks writers while it is held. Removing the exclusion needs
  versioned page visibility and is not implemented yet (ROADMAP item 1).
- Every data page carries a **CRC32**, verified on read, with a directory chain
  that is itself checksummed. Bit rot is detected rather than silently returned as
  missing data. This landed in format version 3 and is documented in `FORMAT.md`.
- A transaction queues its actions in memory until commit, at ≈502 bytes per node
  action and 128 bytes per edge action. It is capped at
  `DEFAULT_MAX_TRANSACTION_ACTIONS` (4,000,000) and reports an error past that rather
  than growing without limit; raise it via
  `GraphLiteOptions::max_transaction_actions` or commit in batches.
- A read snapshot blocks writers while it is held. Keep snapshots short; a
  long-lived snapshot stalls every writer on that database.

## Supported versions

`v1.1.0` is the current stable release. Security fixes are applied to `main` and
backported to the most recent tag; there are no maintained older branches yet.
