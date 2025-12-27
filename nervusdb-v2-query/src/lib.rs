pub mod ast;
pub mod error;
pub mod evaluator;
pub mod executor;
pub mod lexer;
pub mod parser;
pub mod planner;
pub mod query_api;

pub use error::{Error, Result};
pub use executor::{Row, Value, WriteableGraph};
pub use query_api::{Params, PreparedQuery, prepare};

pub fn parse(cypher: &str) -> Result<ast::Query> {
    parser::Parser::parse(cypher)
}
