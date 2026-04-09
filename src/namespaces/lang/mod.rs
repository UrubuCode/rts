mod ast;
mod evaluator;
mod lexer;
mod parser;
mod statement;
mod value;

pub use evaluator::RuntimeContext;
pub use value::JsValue;

use std::collections::HashMap;
use std::thread;

use anyhow::Result;
use crate::namespaces::state::central;

const EXPR_CACHE_MAX_ENTRIES: usize = 512;

fn expr_cache_key() -> String {
    format!("expr_cache_thread_{:?}", thread::current().id())
}

fn with_expr_cache<R>(f: impl FnOnce(&mut HashMap<u64, ast::Expression>) -> R) -> R {
    let cache_state = central().cache::<HashMap<u64, ast::Expression>>(&expr_cache_key());
    let mut guard = cache_state.lock().unwrap();
    f(&mut *guard)
}

fn hash_input(input: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub fn reset_caches() {
    with_expr_cache(|cache| cache.clear());
    statement::reset_script_cache();
}

pub fn evaluate_expression(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    let key = hash_input(input);

    // Try cache first
    let cached = with_expr_cache(|cache| {
        cache.get(&key).cloned()
    });

    if let Some(expression) = cached {
        return evaluator::evaluate(&expression, runtime);
    }

    // Parse and cache
    let tokens = lexer::tokenize(input)?;
    let expression = parser::parse_expression(tokens)?;

    with_expr_cache(|cache| {
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
