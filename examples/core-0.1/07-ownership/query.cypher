MATCH (a:Owner)-[:OWNS]->(b) WHERE a.name = 'Team A' RETURN b.name LIMIT 10
