MATCH (a:Crate)-[:USES]->(b) WHERE a.name = 'nervusdb-cli' RETURN b.name LIMIT 10
