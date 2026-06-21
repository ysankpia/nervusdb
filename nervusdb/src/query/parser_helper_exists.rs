    fn parse_exists_expression(&mut self) -> Result<ExistsExpression, Error> {
        self.consume(&TokenType::LeftBrace, "Expected '{' after EXISTS")?;

        // Check if it's a subquery (starts with MATCH) or a Pattern
        // For T309 we only support Pattern: (n)...
        // But future proofing: if TokenType::Match -> Subquery
        if self.check(&TokenType::Match) {
             // Subquery - not fully supported yet in T309 but we can parse it
             // Let's defer subquery parsing and focus on pattern as per test reqs
             // Actually, the test uses `EXISTS { (n)-... }` which is a pattern without MATCH keyword?
             // Or `EXISTS { MATCH (n)... }`?
             // Standard Cypher `EXISTS { MATCH ... }` is for subqueries.
             // Older/Simple Cypher `EXISTS((n)...)` or `EXISTS { (n)... }` might be pattern.
             // Our test `t309` uses `EXISTS { (n)-[:KNOWS]->() }` -> This is a Pattern (no MATCH keyword).
             let pattern = self.parse_pattern()?;
             self.consume(&TokenType::RightBrace, "Expected '}' after EXISTS pattern")?;
             return Ok(ExistsExpression::Pattern(pattern));
        }

        // Default to Pattern
        let pattern = self.parse_pattern()?;
        self.consume(&TokenType::RightBrace, "Expected '}' after EXISTS pattern")?;
        Ok(ExistsExpression::Pattern(pattern))
    }
