CREATE (a:Service {name: 'worker'})-[:CALLS]->(b:Service {name: 'queue'})
