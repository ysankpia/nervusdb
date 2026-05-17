CREATE (a:File {path: 'src/main.rs'})-[:IMPORTS]->(b:File {path: 'src/db.rs'})
