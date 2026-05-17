MATCH (a:User)-[:LIKES]->(b)-[:IN_CATEGORY]->(c) WHERE a.name = 'Ada' RETURN c.name LIMIT 10
