MATCH (a:TreeNode)-[:PARENT_OF]->(b) WHERE a.name = 'root' RETURN b.name LIMIT 10
