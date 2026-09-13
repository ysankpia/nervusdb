# Concurrency: what each model allows, and what to do about it

This answers one question asked in concrete terms: **"I open three session windows and
they all write to the same database. Does that block?"** It documents what NervusDB does
today, what the databases it compares itself to do, and the options — ordered by what
they cost and what they actually buy.

The short answer for NervusDB today is in [section 1](#1-what-nervusdb-does-today). It is not
"slow"; the second writer **cannot open the file at all**.

---

## 1. What NervusDB does today

Measured, not read off the source (`tests/` had a probe for this while the question was
open):

| Scenario                                               | Result                             |
| ------------------------------------------------------ | ---------------------------------- |
| Two **write** handles on one file                      | The second fails: `DatabaseLocked` |
| A **read-only** handle while a write handle is open    | Fails: `DatabaseLocked`            |
| Two **read-only** handles                              | Both succeed (readers coexist)     |
| A **write** handle while a read-only handle is open    | Fails: `DatabaseLocked`            |
| **Four threads** writing through **one shared handle** | All succeed (200/200 writes)       |

So the rule is: **one writer at a time, across processes and within one process;
readers coexist with each other but not with a writer.**

Two consequences that are easy to get wrong:

- **Two session windows are two processes.** They cannot both write, and the second one
  does not queue — it gets an error immediately. There is no `busy_timeout`; `try_lock`
  fails and the error is returned.
- **Threads inside one process are fine** if they share a handle. The last row above is
  the ordinary case for a server or a worker pool: `NervusDb` is `Clone` and cheap to
  clone, and concurrent writers through one handle are serialized by the write lock, not
  rejected.

The lock is a real file lock (`flock` on Unix, `LockFileEx` on Windows) taken on the data
file itself — no sidecar `.lock` file, preserving the two-file invariant. See
`src/lock.rs`.

### Why the read-only case also fails

A read-only handle refuses to open when the WAL still holds unreplayed committed pages.
It cannot replay them (replay writes the main data file), and returning stale data would
be worse than refusing. So a read-only open can fail _even with no writer present_, right
after a write, until someone opens the database read-write once or checkpoints.

The probe for this was written in response to the question and is worth reproducing: a
fresh database that had just been written to rejected the first read-only open until a
`checkpoint()` had run.

---

## 2. What the other databases do

The mechanisms below are well-established. The comparison is about _models_, not
benchmarks, because the question is about what is allowed, not what is fast.

| System                        | Writers                                     | Readers                          | How                                                                                   |
| ----------------------------- | ------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------- |
| **SQLite** (rollback journal) | 1                                           | readers block writer             | File lock; a write excludes readers                                                   |
| **SQLite** (WAL mode)         | **1**                                       | many, concurrent with the writer | WAL; readers read a snapshot of the WAL, one writer appends                           |
| **PostgreSQL**                | **many**                                    | many                             | MVCC: each transaction sees a snapshot; writers touch different rows without blocking |
| **MySQL / InnoDB**            | **many**                                    | many                             | MVCC + row locks; `SELECT ... FOR UPDATE` for explicit locking                        |
| **DuckDB**                    | 1 per process (multi-process single-writer) | many                             | Single-writer file lock, MVCC inside the process                                      |
| **Kùzu**                      | 1 per process for writes                    | many                             | Embedded, single-writer; multiple read-only processes                                 |
| **Neo4j**                     | many                                        | many                             | Client-server: the server owns the file, transactions are routed to it                |
| **NervusDB**                  | **1**                                       | many (but not during a write)    | File lock + WAL                                                                       |

Three shapes, and only three:

1. **Embedded, single writer** — SQLite WAL, DuckDB, Kùzu, NervusDB. The file is opened
   directly; the OS lock arbitrates. Multiple writers require multiple processes to
   coordinate, which a file lock cannot do beyond excluding.
2. **Embedded, multi writer via MVCC in one process** — a library where all writers share
   one process and the engine versions rows/pages internally.
3. **Client-server** — PostgreSQL, MySQL, Neo4j. The server process owns the file, so
   "many writers" is a property of the _server_, not of the storage format. Every writer
   is a client of one process.

This project's own earlier analysis reached the same reading
([`docs/history/PLAN-1.0.md`](history/PLAN-1.0.md), the "多读单写" row): SQLite and Neo4j
had it, DuckDB and Kùzu did not, and NervusDB did not. What has changed since is the
reader column — **NervusDB now allows concurrent readers**, which was a missing capability
at the time. What has not changed is that a _writer_ still excludes readers; that gap is
section 3.3 below.

**NervusDB is shape 1.** So is SQLite. That is the honest comparison: the question "can
two processes write to one file" has the same answer in SQLite's default and WAL modes
(no), and the same answer in PostgreSQL only because PostgreSQL is not a file API.

### The one difference from SQLite WAL that matters most here

SQLite in WAL mode allows **one writer and many readers at the same time**, because
readers read the WAL snapshot while the writer appends. NervusDB currently does not:
readers and the writer exclude each other.

This is exactly ROADMAP item 1 ("versioned page visibility"), and it is the gap that most
affects a "write in the background, watch in the foreground" workload. It is separate
from ROADMAP item 5 (per-frame latching), which is about readers contending with each
other.

---

## 3. The options, in the order they should be considered

Ordered by benefit per unit of risk. The first two are small; the third is the one that
changes what callers can build.

### 3.1 Do nothing, and document it (already done)

Two processes cannot write. If the workload is "one writer, and occasional readers
between writes", this is already sufficient and costs nothing. The failure is loud
(`DatabaseLocked`), not silent, which is the property that matters most.

**Cost: zero.** Keep it as the default regardless of what else is chosen.

### 3.2 Wait instead of failing (`busy_timeout`)

Right now a second writer gets an immediate error. SQLite has the same model but offers
`busy_timeout`, so a caller can say "wait up to N ms and retry" — which turns a hard
failure into a short wait for the common case of brief writes.

This is the **smallest change** on this list: a bounded retry loop around `try_lock`.
It does not make two writers concurrent; it makes one writer _patient_ about the other.

**Cost: small** (a loop plus an option). **Benefit: high for multi-process tools**
(clients, scripts, editors), where writes are short and infrequent. This is what most
users actually want when they say "it shouldn't block".

### 3.3 Versioned page visibility: readers do not block the writer (ROADMAP item 1)

This is SQLite's WAL-mode property, and it is the one that unlocks the workload in the
question's other half — several windows where one writes and others observe. Readers pin
a snapshot; the writer appends; they do not exclude each other.

It is also **the largest remaining piece of work in this repository**, and it changes
recovery and page visibility.

**Cost: large.** **Benefit: high**, and it is the difference between "a file" and "a
database you can watch live".

### 3.4 Multi-writer (MVCC) or a server

Making two _processes_ write concurrently means either:

- **MVCC across processes** — a versioned page store with cross-process coordination.
  This is a rewrite of the storage layer, and it is where SQLite stops too.
- **A server** — one process owns the file, every client connects to it. This is
  PostgreSQL/Neo4j's answer. It abandons the "single embedded file" premise that this
  project is built on, and with it the entire reason the lock is a file lock.

**Cost: very large, and it changes what the product is.** Listed for completeness, not as
a recommendation. The architecture (AGENTS.md §1: single file, no in-memory graph) is
built on the opposite premise, and this option is explicitly out of scope in ROADMAP.

---

## 4. Recommended

**If the concern is "two windows, short writes, shouldn't error out": do 3.2.** It is the
small change that removes the only _surprising_ part of the current behaviour — the
immediate error — and it is what SQLite users expect from a file database.

**If the concern is "one window writes, others watch live": that is 3.3**, and it is
already ROADMAP item 1. It is a large piece of work with a clear goal.

**Neither is 3.5 (per-frame latching, ROADMAP item 5).** That one only matters when
several threads read the _same_ process concurrently; it does nothing for multiple
processes, which is what session windows are. Keeping the three separate matters, because
they are easy to conflate and they have different fixes.

Do **not** do 3.4. The premise is one embedded file; a server is a different product, and
the project has already decided against it.

## 5. A note on how this was decided

Every claim above about NervusDB's own behaviour was measured with a probe rather than
read off the source, because the two are not the same thing: the read-only-open refusal
in section 1 (which happens with _no writer present_) is only visible by running it, and it is
exactly the kind of detail that turns a "should work" into a support question.
