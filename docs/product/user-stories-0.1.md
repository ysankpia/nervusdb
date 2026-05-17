# 0.1 User Stories

Each story proves a small embedded graph workflow. The point is not breadth; the
point is proving that a Rust app can write local graph data, reopen it, and get
trustworthy one-hop or two-hop results.

## 1. Social Graph

- Graph shape: `Person -KNOWS-> Person`.
- Minimal write path: create two `Person` nodes and one `KNOWS` edge.
- Target query: `MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name`.
- Expected proof: rows are identical before and after reopen.

## 2. Dependency Graph

- Graph shape: `Package -DEPENDS_ON-> Package`.
- Minimal write path: insert package nodes with names and dependency edges.
- Target query: direct dependencies for one package.
- Expected proof: traversal returns only the expected dependency package names.

## 3. File / Module Graph

- Graph shape: `File -IMPORTS-> File`.
- Minimal write path: insert files with `path` properties and import edges.
- Target query: imports from a selected file by property equality.
- Expected proof: label scan, property filter, and neighbor traversal compose.

## 4. Local Knowledge Graph

- Graph shape: `Note -LINKS_TO-> Note`.
- Minimal write path: create note nodes and link edges from a local document set.
- Target query: nearby notes for one title or id.
- Expected proof: a local app can traverse notes without a server.

## 5. Parent / Child Hierarchy

- Graph shape: `Node -PARENT_OF-> Node`.
- Minimal write path: create a root and two child nodes.
- Target query: children for one root.
- Expected proof: directed relationship traversal does not return the parent as
  a child.

## 6. Tag Graph

- Graph shape: `Item -TAGGED_AS-> Tag`.
- Minimal write path: insert items, tags, and tag edges.
- Target query: items connected to a given tag.
- Expected proof: label scan plus relationship query returns the tagged items.

## 7. Ownership Graph

- Graph shape: `Owner -OWNS-> Asset`.
- Minimal write path: create owner and asset nodes with properties.
- Target query: assets owned by a named owner.
- Expected proof: property equality selects the owner and traversal returns the
  assets.

## 8. Package Relationship Graph

- Graph shape: `Crate -USES-> Crate`.
- Minimal write path: write a small crate dependency graph through the CLI or
  direct Rust API.
- Target query: crates used by one crate.
- Expected proof: import/write smoke followed by query output.

## 9. Recommendation Seed

- Graph shape: `User -LIKES-> Item -IN_CATEGORY-> Category`.
- Minimal write path: create one user, liked items, and category edges.
- Target query: two-hop path from user to categories.
- Expected proof: two-hop traversal returns deterministic category rows.

## 10. Import Then Query Smoke

- Graph shape: imported nodes and edges from a small fixture.
- Minimal write path: CLI import-style or repeated `v2 write` commands.
- Target query: read back a representative label and relationship.
- Expected proof: local load, query, reopen, and query again all agree.

Stories are accepted only when the write path, query, expected result, and
validation command are documented in the relevant plan, example, or test.
