CREATE (a:User {name: 'Ada'})-[:LIKES]->(b:Item {name: 'Graph DB'})-[:IN_CATEGORY]->(c:Category {name: 'Databases'})
