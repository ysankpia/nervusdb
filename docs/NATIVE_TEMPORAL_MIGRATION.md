# Native Temporal Memory Migration Guide

## Overview

Starting from v0.6.0, NervusDB supports native Rust implementation for temporal memory operations, providing better performance and cross-platform support. This guide explains how to enable and migrate to the native temporal backend.

## Automatic Detection

The native temporal backend is **automatically enabled** when the native addon is available. No code changes are required.

```typescript
import { NervusDB } from 'nervusdb';

const db = await NervusDB.open('my-database.nervusdb');

// Automatically uses native backend if available
// Falls back to TypeScript implementation if not
await db.memory.ingestMessages([...], { conversationId: 'conv-1' });
```

## Checking Native Support

You can verify if the native backend is active:

```typescript
import { loadNativeCore } from 'nervusdb/native/core';

const nativeCore = loadNativeCore();
if (nativeCore) {
  console.log('✅ Native temporal backend available');
} else {
  console.log('⚠️  Using TypeScript fallback');
}
```

## Data Migration

### Fresh Installation

For new databases, no migration is needed. The native backend will be used automatically if available.

### Existing JSON Temporal Data

Existing temporal data stored in `<db>.temporal.json` is **automatically compatible** with the native backend. The data format remains the same.

**Migration steps:**

1. Update to v0.6.0 or later
2. Install native addon (if not already installed)
3. Open your existing database
4. The native backend will read existing JSON data automatically

**No manual migration required!**

## Platform Support

Native addon is available for:

- macOS (Intel & Apple Silicon)
- Linux (x64, glibc)
- Windows (x64)

If the native addon is not available for your platform, NervusDB automatically falls back to the TypeScript implementation.

## Performance Comparison

| Operation      | TypeScript | Native (Rust) | Improvement |
| -------------- | ---------- | ------------- | ----------- |
| Timeline Query | ~5ms       | ~2ms          | 2.5x faster |
| Entity Lookup  | ~3ms       | ~1ms          | 3x faster   |
| Fact Insertion | ~10ms      | ~5ms          | 2x faster   |

_Benchmarks measured on M1 MacBook Pro with 10,000 facts_

## Disabling Native Backend

To force using the TypeScript implementation:

```bash
export NERVUSDB_DISABLE_NATIVE=1
```

Or in code:

```typescript
process.env.NERVUSDB_DISABLE_NATIVE = '1';
```

## Troubleshooting

### Native addon not loading

**Symptom:** Native backend not detected despite addon being installed

**Solution:**

1. Check addon installation:
   ```bash
   ls native/nervusdb-node/npm/
   ```
2. Verify platform compatibility
3. Check for error messages in console

### Performance not improving

**Symptom:** No performance improvement after enabling native backend

**Solution:**

1. Verify native backend is active (see "Checking Native Support")
2. Ensure you're using timeline queries (not direct JSON access)
3. Check that data is persisted (not in-memory only)

### Data inconsistency

**Symptom:** Different results between native and TypeScript implementations

**Solution:**

1. This should not happen - please report as a bug
2. Integration tests verify parity between implementations
3. Temporarily disable native backend and file an issue

## API Compatibility

All temporal memory APIs remain unchanged:

```typescript
// These APIs work identically with both backends
db.memory.ingestMessages(messages, context);
db.memory.timelineBuilder(entityId).predicate('mentions').all();
db.memory.getStore().queryTimeline({ entityId, role: 'subject' });
db.memory.getStore().traceBack(factId);
```

## Further Reading

- [ADR-006: Temporal Memory Graph Query API](architecture/ADR-006-Temporal-Memory-Graph.md)
- [Temporal Memory Benchmarks](benchmarks/temporal-memory.md)
- [Integration Tests](../tests/integration/temporal/native_parity.test.ts)

## Support

If you encounter issues with native temporal backend:

1. Check this guide's troubleshooting section
2. Review [GitHub Issues](https://github.com/nervusdb/nervusdb/issues)
3. File a new issue with:
   - Platform information
   - NervusDB version
   - Error messages
   - Reproduction steps
