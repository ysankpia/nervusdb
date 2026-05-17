# Glossary

## SQLite For Graphs

The product direction for NervusDB 0.1: embedded, local-file, Rust-first graph
storage with crash recovery and a small query surface.

## Embedded Database

A library opened inside the application process. NervusDB 0.1 does not require a
server, daemon, cluster, or network API.

## Mini-Cypher

The deliberately small query subset kept on the 0.1 path. It is not full
openCypher compatibility.

## Core

The path that must work for 0.1: Rust API, local files, WAL recovery, graph
persistence, traversal, Mini-Cypher, and CLI smoke/debug/import support.

## Experimental

Code or scripts that may be useful later but are not allowed to define 0.1
success.

## Frozen

Areas where build and security maintenance are allowed but new capability work
requires a new ADR before 0.1.

## Historical Gate

A validation script or document from the Beta/full-platform phase that remains
available but is not part of the default 0.1 development loop.
