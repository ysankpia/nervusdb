# NervusDB WASM Storage Engine - Usage Guide

**Version**: 1.2.0  
**Status**: Production Ready ✅

---

## Overview

The NervusDB WASM Storage Engine is a high-performance, code-protected storage backend written in Rust and compiled to WebAssembly.

**Key Benefits**:

- 🔒 **Code Protection**: Binary format (extremely hard to reverse engineer)
- ⚡ **Performance**: 33% faster inserts, near-native speed
- 🛡️ **Memory Safety**: Rust guarantees memory safety
- 📦 **Small Size**: Only 119KB WASM binary

---

## Installation

The WASM module is included in the NervusDB package:

```bash
npm install nervusdb
# or
pnpm add nervusdb
# or
yarn add nervusdb
```

---

## Quick Start

### Basic Usage

```javascript
import { StorageEngine } from 'nervusdb/wasm';

// Create a new storage engine
const engine = new StorageEngine();

// Insert triples
engine.insert('Alice', 'knows', 'Bob');
engine.insert('Bob', 'likes', 'Coffee');
engine.insert('Alice', 'likes', 'Tea');

// Query by subject
const aliceRelations = engine.query_by_subject('Alice');
console.log(aliceRelations);
// [
//   { subject: 'Alice', predicate: 'knows', object: 'Bob' },
//   { subject: 'Alice', predicate: 'likes', object: 'Tea' }
// ]

// Query by predicate
const likesRelations = engine.query_by_predicate('likes');
console.log(likesRelations);
// [
//   { subject: 'Bob', predicate: 'likes', object: 'Coffee' },
//   { subject: 'Alice', predicate: 'likes', object: 'Tea' }
// ]

// Get statistics
const stats = engine.get_stats();
console.log(stats);
// {
//   total_triples: 3,
//   insert_count: 3,
//   query_count: 2
// }

// Clean up
engine.free();
```

---

## API Reference

### Constructor

#### `new StorageEngine()`

Creates a new storage engine with default capacity (1024).

```javascript
const engine = new StorageEngine();
```

#### `StorageEngine.withCapacity(capacity)`

Creates a storage engine with custom capacity for better performance with large datasets.

```javascript
// For large datasets
const engine = StorageEngine.withCapacity(10000);
```

**Parameters**:

- `capacity` (number): Initial HashMap capacity

**Returns**: StorageEngine instance

---

### Insert Operations

#### `insert(subject, predicate, object)`

Inserts a single triple into the storage engine.

```javascript
engine.insert('subject', 'predicate', 'object');
```

**Parameters**:

- `subject` (string): The subject of the triple
- `predicate` (string): The predicate/relationship
- `object` (string): The object of the triple

**Returns**: `void`

**Throws**: Error if serialization fails

---

#### `insertBatch(subjects, predicates, objects)`

Batch insert multiple triples for better performance.

```javascript
const subjects = ['Alice', 'Bob', 'Charlie'];
const predicates = ['knows', 'knows', 'knows'];
const objects = ['Bob', 'Charlie', 'Alice'];

const count = engine.insertBatch(subjects, predicates, objects);
console.log(`Inserted ${count} triples`);
```

**Parameters**:

- `subjects` (string[]): Array of subjects
- `predicates` (string[]): Array of predicates
- `objects` (string[]): Array of objects

**Returns**: Number of triples inserted

**Note**: Arrays must have the same length. If lengths differ, only min(lengths) triples are inserted.

---

### Query Operations

#### `query_by_subject(subject)`

Queries all triples with the given subject.

```javascript
const results = engine.query_by_subject('Alice');
```

**Parameters**:

- `subject` (string): The subject to query

**Returns**: Array of triples matching the subject

```typescript
interface Triple {
  subject: string;
  predicate: string;
  object: string;
}
```

---

#### `query_by_predicate(predicate)`

Queries all triples with the given predicate.

```javascript
const results = engine.query_by_predicate('knows');
```

**Parameters**:

- `predicate` (string): The predicate to query

**Returns**: Array of triples matching the predicate

---

### Utility Operations

#### `get_stats()`

Returns statistics about the storage engine.

```javascript
const stats = engine.get_stats();
console.log(`Total: ${stats.total_triples}, Inserts: ${stats.insert_count}`);
```

**Returns**: Statistics object

```typescript
interface Stats {
  total_triples: number; // Total number of triples stored
  insert_count: number; // Number of insert operations performed
  query_count: number; // Number of query operations performed
}
```

---

#### `size()`

Returns the number of triples in the storage engine.

```javascript
const count = engine.size();
console.log(`Storage contains ${count} triples`);
```

**Returns**: Number of triples

---

#### `clear()`

Removes all triples from the storage engine.

```javascript
engine.clear();
console.log(engine.size()); // 0
```

**Returns**: `void`

---

#### `free()`

Frees the WASM memory. **Always call this when done** to prevent memory leaks.

```javascript
engine.free();
```

**Returns**: `void`

**Important**: After calling `free()`, the engine instance cannot be used anymore.

---

## Performance Tips

### 1. Pre-allocate Capacity

For large datasets, pre-allocate capacity to avoid HashMap resizing:

```javascript
// Bad: Uses default capacity (1024)
const engine = new StorageEngine();
for (let i = 0; i < 10000; i++) {
  engine.insert(`s${i}`, 'pred', `o${i}`);
}

// Good: Pre-allocate for 10K records
const engine = StorageEngine.withCapacity(10000);
for (let i = 0; i < 10000; i++) {
  engine.insert(`s${i}`, 'pred', `o${i}`);
}
```

**Performance improvement**: ~15% faster for large datasets

---

### 2. Use Batch Insert

For bulk operations, use `insertBatch` instead of individual inserts:

```javascript
// Bad: Individual inserts
for (let i = 0; i < 1000; i++) {
  engine.insert(`s${i}`, `p${i}`, `o${i}`);
}

// Good: Batch insert
const subjects = Array.from({ length: 1000 }, (_, i) => `s${i}`);
const predicates = Array.from({ length: 1000 }, (_, i) => `p${i}`);
const objects = Array.from({ length: 1000 }, (_, i) => `o${i}`);
engine.insertBatch(subjects, predicates, objects);
```

**Performance improvement**: Less function call overhead

---

### 3. Reuse Engine Instance

Creating a new engine has initialization overhead. Reuse when possible:

```javascript
// Bad: Create new engine for each operation
function processData(data) {
  const engine = new StorageEngine();
  // ... process data
  engine.free();
}

// Good: Create once, reuse
const engine = new StorageEngine();
function processData(data) {
  // ... process data
  engine.clear(); // Clear instead of recreating
}
```

---

### 4. Memory Management

Always free the engine when done:

```javascript
try {
  const engine = new StorageEngine();
  // ... use engine
} finally {
  engine.free(); // Always clean up
}
```

Or use a wrapper:

```javascript
function withEngine(fn) {
  const engine = new StorageEngine();
  try {
    return fn(engine);
  } finally {
    engine.free();
  }
}

// Usage
withEngine((engine) => {
  engine.insert('Alice', 'knows', 'Bob');
  return engine.query_by_subject('Alice');
});
```

---

## Performance Characteristics

### Insert Performance

| Dataset Size | Time   | Throughput    |
| ------------ | ------ | ------------- |
| 1K records   | ~1.1ms | 891K ops/sec  |
| 10K records  | ~11ms  | ~909K ops/sec |
| 100K records | ~110ms | ~909K ops/sec |

**Complexity**: O(1) amortized per insert (HashMap)

---

### Query Performance

| Query Type   | Time (100 queries) | Throughput        |
| ------------ | ------------------ | ----------------- |
| By Subject   | ~32ms              | 3,075 queries/sec |
| By Predicate | ~32ms              | 3,075 queries/sec |

**Complexity**: O(1) lookup + O(n) filtering

---

### Memory Usage

| Dataset Size | Memory Usage |
| ------------ | ------------ |
| 1K records   | ~120KB       |
| 10K records  | ~1.2MB       |
| 100K records | ~12MB        |

**Overhead**: ~2.4× raw data size (includes JSON serialization + HashMap overhead)

---

## Edge Cases & Special Characters

The WASM engine handles edge cases gracefully:

### Special Characters

```javascript
engine.insert('subject with spaces', 'pred:colon', 'obj/slash');
const results = engine.query_by_subject('subject with spaces');
// Works correctly ✅
```

### Unicode

```javascript
engine.insert('主题', '谓语', '宾语');
const results = engine.query_by_subject('主题');
// Full Unicode support ✅
```

### Long Strings

```javascript
const longString = 'a'.repeat(10000);
engine.insert(longString, 'predicate', 'object');
// Handles large strings ✅
```

### Empty Results

```javascript
const results = engine.query_by_subject('nonexistent');
console.log(results); // [] (empty array)
// No errors, returns empty array ✅
```

---

## Error Handling

The WASM engine throws JavaScript errors for invalid operations:

```javascript
try {
  // Invalid: arrays with different lengths
  engine.insertBatch(['a', 'b'], ['p1'], ['o1', 'o2']);
} catch (error) {
  console.error('Insert failed:', error.message);
}
```

**Common Errors**:

- Serialization failure (invalid data)
- Invalid array arguments
- Memory allocation failure

---

## Comparison: WASM vs JavaScript

| Aspect              | WASM              | JavaScript          |
| ------------------- | ----------------- | ------------------- |
| **Code Protection** | ⭐⭐⭐⭐⭐ Binary | ⭐⭐ Source visible |
| **Insert Speed**    | 891K ops/sec      | ~600K ops/sec       |
| **Query Speed**     | 3K ops/sec        | ~3K ops/sec         |
| **Memory Safety**   | ⭐⭐⭐⭐⭐ Rust   | ⭐⭐⭐ GC           |
| **Bundle Size**     | +119KB            | Baseline            |
| **Debugging**       | ⭐⭐ Harder       | ⭐⭐⭐⭐⭐ Easy     |

**When to use WASM**:

- ✅ Code protection critical
- ✅ Large datasets (>1K records)
- ✅ Performance important
- ✅ Production deployment

**When to use JavaScript**:

- ✅ Small datasets (<100 records)
- ✅ Rapid development
- ✅ Easy debugging needed
- ✅ Bundle size critical

---

## Browser Support

**Current Status**: Node.js only (tested)

**Future**: Browser support planned

```javascript
// Will be supported in future
import { StorageEngine } from 'nervusdb/wasm/browser';
```

---

## TypeScript Support

Full TypeScript definitions included:

```typescript
import { StorageEngine, Triple, Stats } from 'nervusdb/wasm';

const engine: StorageEngine = new StorageEngine();

const triples: Triple[] = engine.query_by_subject('Alice');
const stats: Stats = engine.get_stats();
```

---

## Troubleshooting

### "Cannot find module 'nervusdb/wasm'"

Make sure you're importing from the correct path:

```javascript
// Correct
import { StorageEngine } from 'nervusdb/src/wasm/nervusdb_wasm.js';

// Or use package exports (if configured)
import { StorageEngine } from 'nervusdb/wasm';
```

### Memory leak warnings

Always call `free()` when done:

```javascript
const engine = new StorageEngine();
try {
  // ... use engine
} finally {
  engine.free(); // Essential!
}
```

### Performance slower than expected

1. Use `withCapacity()` for large datasets
2. Use `insertBatch()` for bulk operations
3. Reuse engine instances

---

## Examples

### Example 1: Social Network

```javascript
const engine = StorageEngine.withCapacity(1000);

// Build social graph
engine.insert('Alice', 'follows', 'Bob');
engine.insert('Alice', 'follows', 'Charlie');
engine.insert('Bob', 'follows', 'Charlie');
engine.insert('Charlie', 'follows', 'Alice');

// Query: Who does Alice follow?
const aliceFollows = engine.query_by_subject('Alice');
console.log(
  'Alice follows:',
  aliceFollows.map((t) => t.object),
);

// Query: Who follows Charlie?
const charlieFollowers = engine
  .query_by_predicate('follows')
  .filter((t) => t.object === 'Charlie')
  .map((t) => t.subject);
console.log('Charlie is followed by:', charlieFollowers);

engine.free();
```

### Example 2: Knowledge Base

```javascript
const engine = new StorageEngine();

// Add facts
engine.insert('Dog', 'is_a', 'Animal');
engine.insert('Dog', 'has', 'Tail');
engine.insert('Cat', 'is_a', 'Animal');
engine.insert('Cat', 'has', 'Whiskers');

// Query: What is a Dog?
const dogFacts = engine.query_by_subject('Dog');
console.log('Dog facts:', dogFacts);

// Query: What has tails?
const hasTail = engine
  .query_by_predicate('has')
  .filter((t) => t.object === 'Tail')
  .map((t) => t.subject);
console.log('Has tail:', hasTail);

engine.free();
```

### Example 3: Bulk Import

```javascript
const engine = StorageEngine.withCapacity(10000);

// Import CSV data
const csvData = `
Alice,knows,Bob
Bob,knows,Charlie
Charlie,knows,Alice
`
  .trim()
  .split('\n');

const subjects = [];
const predicates = [];
const objects = [];

for (const line of csvData) {
  const [s, p, o] = line.split(',');
  subjects.push(s);
  predicates.push(p);
  objects.push(o);
}

const count = engine.insertBatch(subjects, predicates, objects);
console.log(`Imported ${count} triples`);

engine.free();
```

---

## FAQ

**Q: Is the WASM engine thread-safe?**  
A: No, it's designed for single-threaded use. Use separate instances for different threads.

**Q: Can I persist data to disk?**  
A: Not currently. The WASM engine is in-memory only. Future versions may add persistence.

**Q: What's the maximum dataset size?**  
A: Limited by available memory. Tested up to 100K triples without issues.

**Q: Does it work in browsers?**  
A: Not tested yet. Node.js only for now. Browser support is planned.

**Q: Can I use it with NervusDB's main API?**  
A: Currently separate. Future integration planned.

---

## Support

For issues and questions:

- GitHub Issues: https://github.com/JdPrect/NervusDB/issues
- Documentation: https://github.com/JdPrect/NervusDB/tree/main/docs

---

**Version**: 1.2.0  
**Last Updated**: 2025-01-14  
**Status**: Production Ready ✅
