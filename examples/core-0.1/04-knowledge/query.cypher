MATCH (a:Note)-[:LINKS_TO]->(b) WHERE a.title = 'Graph Storage' RETURN b.title LIMIT 10
