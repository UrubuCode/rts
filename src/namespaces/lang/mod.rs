mod ast;
mod evaluator;
mod lexer;
mod parser;
mod statement;
mod value;

pub use evaluator::RuntimeContext;
pub use value::JsValue;

use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::Result;

const EXPR_CACHE_MAX_ENTRIES: usize = 512;

// Optimized: back to thread_local for better performance
thread_local! {
    static EXPR_CACHE: RefCell<HashMap<u64, ast::Expression>> = RefCell::new(HashMap::new());
}

fn hash_input(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub fn reset_caches() {
    EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
    statement::reset_script_cache();
}

pub fn evaluate_expression(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    let key = hash_input(input);

    // Try cache first
    let cached = EXPR_CACHE.with(|cache| {
        cache.borrow().get(&key).cloned()
    });

    if let Some(expression) = cached {
        return evaluator::evaluate(&expression, runtime);
    }

    // Parse and cache
    let tokens = lexer::tokenize(input)?;
    let expression = parser::parse_expression(tokens)?;

    EXPR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= EXPR_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(key, expression.clone());
    });

    evaluator::evaluate(&expression, runtime)
}

pub fn evaluate_statement(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    statement::evaluate_statement(input, runtime)
}
