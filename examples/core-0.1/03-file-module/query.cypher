MATCH (a:File)-[:IMPORTS]->(b) WHERE a.path = 'src/main.rs' RETURN b.path LIMIT 10
