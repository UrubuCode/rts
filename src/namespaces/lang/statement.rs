use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use swc_common::{FileName, SourceMap, SourceMapper, Span as SwcSpan, Spanned, sync::Lrc};
use swc_ecma_ast::{
    AssignOp, AssignTarget, BlockStmt, Decl, Expr, ForHead, Pat, Script, SimpleAssignTarget,
    Stmt, UpdateOp, VarDecl, VarDeclKind, VarDeclOrExpr,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use super::{JsValue, RuntimeContext, evaluate_expression};

const SCRIPT_CACHE_MAX_ENTRIES: usize = 256;

struct CachedScript {
    cm: Lrc<SourceMap>,
    script: Script,
}

// NOTE: Cannot migrate to central state - Lrc<SourceMap> does not implement std Send/Sync
// swc uses custom Send/Sync traits which are incompatible with central state requirements
thread_local! {
    static SCRIPT_CACHE: RefCell<HashMap<u64, CachedScript>> = RefCell::new(HashMap::new());
}

fn hash_source(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

pub fn reset_script_cache() {
    SCRIPT_CACHE.with(|cache| cache.borrow_mut().clear());
}

pub fn evaluate_statement(input: &str, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    let key = hash_source(input);

    // Try cache first
    let cached = SCRIPT_CACHE.with(|cache| {
        cache.borrow().get(&key).map(|entry| {
            (entry.cm.clone(), entry.script.clone())
        })
    });

    if let Some((cm, script)) = cached {
        return execute_script(&script, cm.as_ref(), runtime);
    }

    // Parse and cache
    let parsed = parse_script(input)?;
    let cm = parsed.cm.clone();
    let script = parsed.script.clone();

    SCRIPT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= SCRIPT_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(key, CachedScript { cm: parsed.cm, script: parsed.script });
    });

    execute_script(&script, cm.as_ref(), runtime)
}

struct ParsedScript {
    cm: Lrc<SourceMap>,
    script: Script,
}

fn parse_script(source: &str) -> Result<ParsedScript> {
    let syntax_order = [ts_syntax(), es_syntax()];
    let mut first_error = None::<String>;

    for syntax in syntax_order {
        match parse_script_with_syntax(source, syntax) {
            Ok(parsed) => return Ok(parsed),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.to_string());
                }
            }
        }
    }

    Err(anyhow!(
        "failed to parse statement/script: {}",
        first_error.unwrap_or_else(|| "unknown parser error".to_string())
    ))
}

fn parse_script_with_syntax(source: &str, syntax: Syntax) -> Result<ParsedScript> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-runtime-statement.ts".into())),
        source.to_string(),
    );

    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);

    let script = parser
        .parse_script()
        .map_err(|error| anyhow!(format_parser_error(&cm, &error)))?;

    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(anyhow!(format_parser_error(&cm, &error)));
    }

    Ok(ParsedScript { cm, script })
}

fn format_parser_error(cm: &Lrc<SourceMap>, error: &swc_ecma_parser::error::Error) -> String {
    let message = error.kind().msg();
    let span = error.span();
    if span.is_dummy() {
        return message.into_owned();
    }

    let loc = cm.lookup_char_pos(span.lo());
    format!(
        "{} at {}:{}",
        message,
        loc.line,
        loc.col_display.saturating_add(1)
    )
}

fn ts_syntax() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: false,
        decorators: true,
        ..Default::default()
    })
}

fn es_syntax() -> Syntax {
    Syntax::Es(EsSyntax {
        jsx: false,
        decorators: true,
        ..Default::default()
    })
}

#[derive(Debug, Clone)]
enum Step {
    Next(JsValue),
    Break,
    Continue,
    Return(JsValue),
    Throw(JsValue),
}

fn execute_script(
    script: &Script,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<JsValue> {
    let mut last = JsValue::Undefined;
    for statement in &script.body {
        match execute_statement(statement, cm, runtime)? {
            Step::Next(value) => last = value,
            Step::Return(value) => return Ok(value),
            Step::Break => bail!("break used outside loop"),
            Step::Continue => bail!("continue used outside loop"),
            Step::Throw(value) => bail!("uncaught runtime throw: {}", value.to_js_string()),
        }
    }
    Ok(last)
}

fn execute_statement(
    statement: &Stmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    match statement {
        Stmt::Empty(_) => Ok(Step::Next(JsValue::Undefined)),
        Stmt::Expr(expr_stmt) => Ok(Step::Next(evaluate_expr(&expr_stmt.expr, cm, runtime)?)),
        Stmt::Decl(Decl::Var(var_decl)) => Ok(Step::Next(execute_var_decl(var_decl, cm, runtime)?)),
        Stmt::Block(block) => execute_block(block, cm, runtime),
        Stmt::If(if_stmt) => execute_if_statement(if_stmt, cm, runtime),
        Stmt::Switch(switch_stmt) => execute_switch_statement(switch_stmt, cm, runtime),
        Stmt::While(while_stmt) => execute_while_statement(while_stmt, cm, runtime),
        Stmt::DoWhile(do_while_stmt) => execute_do_while_statement(do_while_stmt, cm, runtime),
        Stmt::For(for_stmt) => execute_for_statement(for_stmt, cm, runtime),
        Stmt::ForIn(for_in_stmt) => execute_for_in_statement(for_in_stmt, cm, runtime),
        Stmt::ForOf(for_of_stmt) => execute_for_of_statement(for_of_stmt, cm, runtime),
        Stmt::Try(try_stmt) => execute_try_statement(try_stmt, cm, runtime),
        Stmt::Throw(throw_stmt) => {
            let value = evaluate_expr(&throw_stmt.arg, cm, runtime)?;
            Ok(Step::Throw(value))
        }
        Stmt::Break(_) => Ok(Step::Break),
        Stmt::Continue(_) => Ok(Step::Continue),
        Stmt::Return(return_stmt) => {
            let value = if let Some(argument) = &return_stmt.arg {
                evaluate_expr(argument, cm, runtime)?
            } else {
                JsValue::Undefined
            };
            Ok(Step::Return(value))
        }
        _ => {
            let snippet = span_snippet(cm, statement.span()).unwrap_or_else(|| "<unknown>".to_string());
            bail!("unsupported statement in runtime evaluator: {}", snippet.trim());
        }
    }
}

fn execute_block(block: &BlockStmt, cm: &SourceMap, runtime: &mut dyn RuntimeContext) -> Result<Step> {
    let mut last = JsValue::Undefined;
    for statement in &block.stmts {
        match execute_statement(statement, cm, runtime)? {
            Step::Next(value) => last = value,
            Step::Break => return Ok(Step::Break),
            Step::Continue => return Ok(Step::Continue),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        }
    }
    Ok(Step::Next(last))
}

fn execute_if_statement(
    if_stmt: &swc_ecma_ast::IfStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    let test = evaluate_expr(&if_stmt.test, cm, runtime)?;
    if test.truthy() {
        execute_statement(&if_stmt.cons, cm, runtime)
    } else if let Some(alternate) = &if_stmt.alt {
        execute_statement(alternate, cm, runtime)
    } else {
        Ok(Step::Next(JsValue::Undefined))
    }
}

fn execute_switch_statement(
    switch_stmt: &swc_ecma_ast::SwitchStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    let discriminant = evaluate_expr(&switch_stmt.discriminant, cm, runtime)?;
    let mut matched = false;
    let mut last = JsValue::Undefined;

    for case in &switch_stmt.cases {
        if !matched {
            matched = if let Some(test) = &case.test {
                let case_value = evaluate_expr(test, cm, runtime)?;
                strict_equal(&discriminant, &case_value)
            } else {
                true
            };
        }

        if !matched {
            continue;
        }

        for statement in &case.cons {
            match execute_statement(statement, cm, runtime)? {
                Step::Next(value) => last = value,
                Step::Break => return Ok(Step::Next(last)),
                Step::Continue => return Ok(Step::Continue),
                Step::Return(value) => return Ok(Step::Return(value)),
                Step::Throw(value) => return Ok(Step::Throw(value)),
            }
        }
    }

    Ok(Step::Next(last))
}

fn execute_while_statement(
    while_stmt: &swc_ecma_ast::WhileStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    loop {
        let test = evaluate_expr(&while_stmt.test, cm, runtime)?;
        if !test.truthy() {
            return Ok(Step::Next(JsValue::Undefined));
        }

        match execute_statement(&while_stmt.body, cm, runtime)? {
            Step::Next(_) | Step::Continue => {}
            Step::Break => return Ok(Step::Next(JsValue::Undefined)),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        }
    }
}

fn execute_do_while_statement(
    do_while_stmt: &swc_ecma_ast::DoWhileStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    loop {
        match execute_statement(&do_while_stmt.body, cm, runtime)? {
            Step::Next(_) | Step::Continue => {}
            Step::Break => return Ok(Step::Next(JsValue::Undefined)),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        }

        let test = evaluate_expr(&do_while_stmt.test, cm, runtime)?;
        if !test.truthy() {
            return Ok(Step::Next(JsValue::Undefined));
        }
    }
}

fn execute_for_statement(
    for_stmt: &swc_ecma_ast::ForStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    if let Some(initializer) = &for_stmt.init {
        match initializer {
            VarDeclOrExpr::VarDecl(var_decl) => {
                let _ = execute_var_decl(var_decl, cm, runtime)?;
            }
            VarDeclOrExpr::Expr(expr) => {
                let _ = evaluate_expr(expr, cm, runtime)?;
            }
        }
    }

    loop {
        if let Some(test) = &for_stmt.test {
            let value = evaluate_expr(test, cm, runtime)?;
            if !value.truthy() {
                return Ok(Step::Next(JsValue::Undefined));
            }
        }

        let should_continue = match execute_statement(&for_stmt.body, cm, runtime)? {
            Step::Next(_) => true,
            Step::Continue => true,
            Step::Break => return Ok(Step::Next(JsValue::Undefined)),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        };

        if should_continue {
            if let Some(update) = &for_stmt.update {
                let _ = evaluate_expr(update, cm, runtime)?;
            }
        }
    }
}

fn execute_for_in_statement(
    for_in_stmt: &swc_ecma_ast::ForInStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    let binding = for_head_binding_name(&for_in_stmt.left)?;
    let right = evaluate_expr(&for_in_stmt.right, cm, runtime)?;
    let keys = enumerable_keys(right);

    for key in keys {
        let _ = runtime.write_identifier(binding.as_str(), JsValue::String(key))?;
        match execute_statement(&for_in_stmt.body, cm, runtime)? {
            Step::Next(_) | Step::Continue => {}
            Step::Break => return Ok(Step::Next(JsValue::Undefined)),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        }
    }

    Ok(Step::Next(JsValue::Undefined))
}

fn execute_for_of_statement(
    for_of_stmt: &swc_ecma_ast::ForOfStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    let binding = for_head_binding_name(&for_of_stmt.left)?;
    let right = evaluate_expr(&for_of_stmt.right, cm, runtime)?;
    let values = iterable_values(right)?;

    for value in values {
        let _ = runtime.write_identifier(binding.as_str(), value)?;
        match execute_statement(&for_of_stmt.body, cm, runtime)? {
            Step::Next(_) | Step::Continue => {}
            Step::Break => return Ok(Step::Next(JsValue::Undefined)),
            Step::Return(value) => return Ok(Step::Return(value)),
            Step::Throw(value) => return Ok(Step::Throw(value)),
        }
    }

    Ok(Step::Next(JsValue::Undefined))
}

fn execute_try_statement(
    try_stmt: &swc_ecma_ast::TryStmt,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<Step> {
    let primary = execute_block(&try_stmt.block, cm, runtime)?;
    let mut outcome = match primary {
        Step::Throw(value) => {
            if let Some(handler) = &try_stmt.handler {
                if let Some(param) = &handler.param {
                    bind_pattern(param, value, true, runtime)?;
                }
                execute_block(&handler.body, cm, runtime)?
            } else {
                Step::Throw(value)
            }
        }
        other => other,
    };

    if let Some(finalizer) = &try_stmt.finalizer {
        let finalizer_step = execute_block(finalizer, cm, runtime)?;
        outcome = match finalizer_step {
            Step::Next(_) => outcome,
            Step::Break => Step::Break,
            Step::Continue => Step::Continue,
            Step::Return(value) => Step::Return(value),
            Step::Throw(value) => Step::Throw(value),
        };
    }

    Ok(outcome)
}

fn execute_var_decl(
    var_decl: &VarDecl,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<JsValue> {
    let mutable = var_decl.kind != VarDeclKind::Const;
    let mut last = JsValue::Undefined;

    for declarator in &var_decl.decls {
        let value = if let Some(initializer) = &declarator.init {
            evaluate_expr(initializer, cm, runtime)?
        } else {
            JsValue::Undefined
        };

        bind_pattern(&declarator.name, value.clone(), mutable, runtime)?;
        last = value;
    }

    Ok(last)
}

fn bind_pattern(
    pattern: &Pat,
    value: JsValue,
    mutable: bool,
    runtime: &mut dyn RuntimeContext,
) -> Result<()> {
    match pattern {
        Pat::Ident(binding) => {
            let _ = runtime.define_identifier(binding.id.sym.as_ref(), value, mutable)?;
            Ok(())
        }
        _ => bail!("unsupported declaration pattern in runtime evaluator"),
    }
}

fn evaluate_expr(expr: &Expr, cm: &SourceMap, runtime: &mut dyn RuntimeContext) -> Result<JsValue> {
    let resolved = resolve_runtime_expr(expr);

    match resolved {
        Expr::Assign(assign) => execute_assign_expr(assign, cm, runtime),
        Expr::Update(update) => execute_update_expr(update, cm, runtime),
        _ => {
            let snippet = span_snippet(cm, resolved.span())
                .ok_or_else(|| anyhow!("failed to read runtime expression snippet"))?;
            evaluate_expression(snippet.trim(), runtime)
        }
    }
}

fn resolve_runtime_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren) => resolve_runtime_expr(&paren.expr),
        Expr::TsAs(value) => resolve_runtime_expr(&value.expr),
        Expr::TsSatisfies(value) => resolve_runtime_expr(&value.expr),
        Expr::TsNonNull(value) => resolve_runtime_expr(&value.expr),
        Expr::TsTypeAssertion(value) => resolve_runtime_expr(&value.expr),
        Expr::TsInstantiation(value) => resolve_runtime_expr(&value.expr),
        _ => expr,
    }
}

fn execute_assign_expr(
    assign: &swc_ecma_ast::AssignExpr,
    cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<JsValue> {
    let name = assign_target_identifier(&assign.left)
        .ok_or_else(|| anyhow!("unsupported assignment target in runtime evaluator"))?;
    let right = evaluate_expr(&assign.right, cm, runtime)?;

    let assigned = match assign.op {
        AssignOp::Assign => right,
        AssignOp::AddAssign => add_values(runtime.read_identifier(name.as_str()), right),
        AssignOp::SubAssign => number_op(runtime.read_identifier(name.as_str()), right, |a, b| a - b),
        AssignOp::MulAssign => number_op(runtime.read_identifier(name.as_str()), right, |a, b| a * b),
        AssignOp::DivAssign => number_op(runtime.read_identifier(name.as_str()), right, |a, b| a / b),
        AssignOp::ModAssign => number_op(runtime.read_identifier(name.as_str()), right, |a, b| a % b),
        _ => bail!("unsupported assignment operator in runtime evaluator"),
    };

    runtime.write_identifier(name.as_str(), assigned.clone())
}

fn execute_update_expr(
    update: &swc_ecma_ast::UpdateExpr,
    _cm: &SourceMap,
    runtime: &mut dyn RuntimeContext,
) -> Result<JsValue> {
    let target_name = expr_identifier(&update.arg)
        .ok_or_else(|| anyhow!("unsupported update target in runtime evaluator"))?;
    let current = runtime
        .read_identifier(target_name.as_str())
        .unwrap_or(JsValue::Undefined)
        .to_number();
    let next = match update.op {
        UpdateOp::PlusPlus => current + 1.0,
        UpdateOp::MinusMinus => current - 1.0,
    };

    let previous_value = JsValue::Number(current);
    let updated_value = JsValue::Number(next);
    let _ = runtime.write_identifier(target_name.as_str(), updated_value.clone())?;

    if update.prefix {
        Ok(updated_value)
    } else {
        Ok(previous_value)
    }
}

fn add_values(current: Option<JsValue>, rhs: JsValue) -> JsValue {
    let lhs = current.unwrap_or(JsValue::Undefined);
    if lhs.is_string_like() || rhs.is_string_like() {
        JsValue::String(format!("{}{}", lhs.to_js_string(), rhs.to_js_string()))
    } else {
        JsValue::Number(lhs.to_number() + rhs.to_number())
    }
}

fn number_op(current: Option<JsValue>, rhs: JsValue, op: impl Fn(f64, f64) -> f64) -> JsValue {
    let lhs = current.unwrap_or(JsValue::Undefined).to_number();
    JsValue::Number(op(lhs, rhs.to_number()))
}

fn enumerable_keys(value: JsValue) -> Vec<String> {
    match value {
        JsValue::Object(map) => map.into_keys().collect(),
        JsValue::String(text) => text
            .chars()
            .enumerate()
            .map(|(index, _)| index.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn iterable_values(value: JsValue) -> Result<Vec<JsValue>> {
    match value {
        JsValue::String(text) => Ok(text
            .chars()
            .map(|ch| JsValue::String(ch.to_string()))
            .collect()),
        JsValue::Object(map) => Ok(map.into_values().collect()),
        JsValue::Null | JsValue::Undefined => bail!("cannot iterate over nullish value"),
        _ => bail!("value is not iterable in runtime evaluator"),
    }
}

fn for_head_binding_name(head: &ForHead) -> Result<String> {
    match head {
        ForHead::VarDecl(var_decl) => {
            if var_decl.decls.len() != 1 {
                bail!("for-loop declaration head supports a single binding");
            }
            binding_name_from_pattern(&var_decl.decls[0].name)
        }
        ForHead::Pat(pattern) => binding_name_from_pattern(pattern),
        ForHead::UsingDecl(_) => bail!("using declarations are not supported in runtime evaluator"),
    }
}

fn binding_name_from_pattern(pattern: &Pat) -> Result<String> {
    match pattern {
        Pat::Ident(binding) => Ok(binding.id.sym.to_string()),
        _ => bail!("unsupported loop binding pattern in runtime evaluator"),
    }
}

fn strict_equal(lhs: &JsValue, rhs: &JsValue) -> bool {
    match (lhs, rhs) {
        (JsValue::Number(a), JsValue::Number(b)) => a == b,
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
        (JsValue::Object(a), JsValue::Object(b)) => a == b,
        (JsValue::NativeFunction(a), JsValue::NativeFunction(b)) => a == b,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Undefined, JsValue::Undefined) => true,
        _ => false,
    }
}

fn assign_target_identifier(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => match simple {
            SimpleAssignTarget::Ident(binding) => Some(binding.id.sym.to_string()),
            SimpleAssignTarget::Paren(paren) => expr_identifier(&paren.expr),
            _ => None,
        },
        AssignTarget::Pat(_) => None,
    }
}

fn expr_identifier(expr: &Expr) -> Option<String> {
    match resolve_runtime_expr(expr) {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

fn span_snippet(cm: &SourceMap, span: SwcSpan) -> Option<String> {
    if span.is_dummy() {
        return None;
    }
    cm.span_to_snippet(span).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Result;

    use crate::namespaces::lang::{JsValue, RuntimeContext};

    use super::evaluate_statement;

    #[derive(Default)]
    struct TestRuntime {
        vars: BTreeMap<String, (JsValue, bool)>,
    }

    impl RuntimeContext for TestRuntime {
        fn read_identifier(&self, name: &str) -> Option<JsValue> {
            self.vars.get(name).map(|(value, _)| value.clone())
        }

        fn call_function(&mut self, _callee: &str, _args: Vec<JsValue>) -> Result<JsValue> {
            Ok(JsValue::Undefined)
        }

        fn define_identifier(
            &mut self,
            name: &str,
            value: JsValue,
            mutable: bool,
        ) -> Result<JsValue> {
            if let Some((existing, existing_mutable)) = self.vars.get(name).cloned() {
                if !existing_mutable {
                    return Ok(existing);
                }
            }
            self.vars.insert(name.to_string(), (value.clone(), mutable));
            Ok(value)
        }

        fn write_identifier(&mut self, name: &str, value: JsValue) -> Result<JsValue> {
            if let Some((_, mutable)) = self.vars.get(name).cloned() {
                if !mutable {
                    return Ok(value);
                }
            }
            self.vars.insert(name.to_string(), (value.clone(), true));
            Ok(value)
        }
    }

    #[test]
    fn executes_if_else_if_else_chain() {
        let mut runtime = TestRuntime::default();
        let result = evaluate_statement(
            r#"
            let valor = 0;
            if (false) {
                valor = 1;
            } else if (true) {
                valor = 2;
            } else {
                valor = 3;
            }
            valor;
        "#,
            &mut runtime,
        )
        .expect("if/else chain should evaluate");

        assert_eq!(result, JsValue::Number(2.0));
    }

    #[test]
    fn executes_for_while_and_do_while_loops() {
        let mut runtime = TestRuntime::default();
        let result = evaluate_statement(
            r#"
            let total = 0;
            for (let i = 0; i < 3; i++) {
                total = total + 1;
            }
            while (total < 5) {
                total = total + 1;
            }
            do {
                total = total + 1;
            } while (total < 6);
            total;
        "#,
            &mut runtime,
        )
        .expect("loops should evaluate");

        assert_eq!(result, JsValue::Number(6.0));
    }

    #[test]
    fn executes_switch_and_try_catch_finally() {
        let mut runtime = TestRuntime::default();
        let result = evaluate_statement(
            r#"
            let state = 0;
            switch (2) {
                case 1:
                    state = 1;
                    break;
                case 2:
                    state = 2;
                    break;
                default:
                    state = 9;
            }
            try {
                throw "x";
            } catch (err) {
                if (err === "x") {
                    state = state + 1;
                }
            } finally {
                state = state + 1;
            }
            state;
        "#,
            &mut runtime,
        )
        .expect("switch and try/catch/finally should evaluate");

        assert_eq!(result, JsValue::Number(4.0));
    }

    #[test]
    fn executes_for_in_and_for_of_on_object_values() {
        let mut runtime = TestRuntime::default();
        let mut source = BTreeMap::new();
        source.insert("a".to_string(), JsValue::Number(1.0));
        source.insert("b".to_string(), JsValue::Number(2.0));
        runtime.vars.insert(
            "source".to_string(),
            (JsValue::Object(source), true),
        );

        let result = evaluate_statement(
            r#"
            let keyCount = 0;
            for (const key in source) {
                keyCount = keyCount + 1;
            }
            let valueSum = 0;
            for (const value of source) {
                valueSum = valueSum + value;
            }
            keyCount + valueSum;
        "#,
            &mut runtime,
        )
        .expect("for-in and for-of should evaluate for object values");

        assert_eq!(result, JsValue::Number(5.0));
    }
}
