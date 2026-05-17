MATCH (a:Person)-[:KNOWS]->(b) WHERE a.name = 'Alice' RETURN b.name LIMIT 10
