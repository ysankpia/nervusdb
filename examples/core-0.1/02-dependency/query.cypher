MATCH (a:Package)-[:DEPENDS_ON]->(b) WHERE a.name = 'app' RETURN b.name LIMIT 10
