MATCH (a:Service)-[:CALLS]->(b) WHERE a.name = 'api' RETURN b.name LIMIT 10
