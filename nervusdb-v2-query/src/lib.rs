pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod planner;

pub use error::{Error, Result};

pub fn parse(cypher: &str) -> Result<ast::Query> {
    parser::Parser::parse(cypher)
}
