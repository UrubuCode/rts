mod ast;
mod evaluator;
mod lexer;
mod parser;
mod statement;
mod value;

pub use evaluator::RuntimeContext;
pub use value::JsValue;

use anyhow::Result;

pub fn evaluate_expression(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    let tokens = lexer::tokenize(input)?;
    let expression = parser::parse_expression(tokens)?;
    evaluator::evaluate(&expression, runtime)
}

pub fn evaluate_statement(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    statement::evaluate_statement(input, runtime)
}
