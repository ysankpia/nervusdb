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

- Only one handle may open a database at a time; there is no read-only
  multi-reader mode.
- The data file has **no page checksums**. Structure is validated by
  `integrity_check`, but bit rot that stays structurally consistent is not
  detected. Fixing this is a storage format change.
- A single very large transaction holds its whole action list in memory. Chunk
  huge writes.

## Supported versions

This is a pre-release (`v1.0.0-rc.2`). Security fixes are applied to `main` and
to the most recent tag; there are no maintained older branches yet. Once 1.0 is
released, this section will list which series receive fixes.
