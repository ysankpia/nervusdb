MATCH (a:Item)-[:TAGGED_AS]->(b) WHERE b.name = 'work' RETURN a.name LIMIT 10
