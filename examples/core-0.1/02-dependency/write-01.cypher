CREATE (a:Package {name: 'app'})-[:DEPENDS_ON]->(b:Package {name: 'serde'})
