use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rustc_hash::FxHashMap;

use crate::namespaces::DispatchOutcome;
use crate::namespaces::abi;
use crate::namespaces::value::RuntimeValue;
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

#[derive(Debug, Default, Clone)]
struct EvalMetrics {
    parse_calls: u64,
    parse_nanos: u128,
    eval_expr_calls: u64,
    eval_expr_nanos: u128,
    eval_stmt_calls: u64,
    eval_stmt_nanos: u128,
    identifier_reads: u64,
    identifier_writes: u64,
    call_dispatches: u64,
    binding_cache_hits: u64,
    binding_cache_misses: u64,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EvalMetricsSnapshot {
    pub parse_calls: u64,
    pub parse_nanos: u128,
    pub identifier_reads: u64,
    pub identifier_writes: u64,
    pub call_dispatches: u64,
    pub binding_cache_hits: u64,
    pub binding_cache_misses: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct EvalLocalMetrics {
    identifier_reads: u64,
    identifier_writes: u64,
    call_dispatches: u64,
    binding_cache_hits: u64,
    binding_cache_misses: u64,
}

#[derive(Debug, Default)]
struct EvalContext {
    binding_cache: FxHashMap<String, abi::RuntimeBinding>,
    metrics: EvalLocalMetrics,
    collect_metrics: bool,
}

impl EvalContext {
    fn new() -> Self {
        Self {
            binding_cache: FxHashMap::default(),
            metrics: EvalLocalMetrics::default(),
            collect_metrics: metrics_enabled(),
        }
    }

    #[inline(always)]
    fn record_identifier_read(&mut self) {
        if self.collect_metrics {
            self.metrics.identifier_reads = self.metrics.identifier_reads.saturating_add(1);
        }
    }

    #[inline(always)]
    fn record_identifier_write(&mut self) {
        if self.collect_metrics {
            self.metrics.identifier_writes = self.metrics.identifier_writes.saturating_add(1);
        }
    }

    #[inline(always)]
    fn record_call_dispatch(&mut self) {
        if self.collect_metrics {
            self.metrics.call_dispatches = self.metrics.call_dispatches.saturating_add(1);
        }
    }

    #[inline(always)]
    fn record_binding_cache_hit(&mut self) {
        if self.collect_metrics {
            self.metrics.binding_cache_hits = self.metrics.binding_cache_hits.saturating_add(1);
        }
    }

    #[inline(always)]
    fn record_binding_cache_miss(&mut self) {
        if self.collect_metrics {
            self.metrics.binding_cache_misses = self.metrics.binding_cache_misses.saturating_add(1);
        }
    }

    fn flush_metrics(self) {
        if !self.collect_metrics {
            return;
        }
        with_metrics(|metrics| {
            metrics.identifier_reads = metrics
                .identifier_reads
                .saturating_add(self.metrics.identifier_reads);
            metrics.identifier_writes = metrics
                .identifier_writes
                .saturating_add(self.metrics.identifier_writes);
            metrics.call_dispatches = metrics
                .call_dispatches
                .saturating_add(self.metrics.call_dispatches);
            metrics.binding_cache_hits = metrics
                .binding_cache_hits
                .saturating_add(self.metrics.binding_cache_hits);
            metrics.binding_cache_misses = metrics
                .binding_cache_misses
                .saturating_add(self.metrics.binding_cache_misses);
        });
    }
}

thread_local! {
    static EVAL_METRICS: RefCell<EvalMetrics> = RefCell::new(EvalMetrics::default());
}

static METRICS_ENABLED: AtomicBool = AtomicBool::new(false);

#[inline(always)]
fn metrics_enabled() -> bool {
    METRICS_ENABLED.load(Ordering::Relaxed)
}

#[inline(always)]
fn with_metrics(update: impl FnOnce(&mut EvalMetrics)) {
    if !metrics_enabled() {
        return;
    }
    EVAL_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        update(&mut metrics);
    });
}

pub(crate) fn set_metrics_enabled(enabled: bool) {
    METRICS_ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        reset_metrics();
    }
}

pub(crate) fn reset_metrics() {
    EVAL_METRICS.with(|metrics| {
        *metrics.borrow_mut() = EvalMetrics::default();
    });
}

pub(crate) fn metrics_snapshot() -> EvalMetricsSnapshot {
    EVAL_METRICS.with(|metrics| {
        let metrics = metrics.borrow();
        EvalMetricsSnapshot {
            parse_calls: metrics.parse_calls,
            parse_nanos: metrics.parse_nanos,
            identifier_reads: metrics.identifier_reads,
            identifier_writes: metrics.identifier_writes,
            call_dispatches: metrics.call_dispatches,
            binding_cache_hits: metrics.binding_cache_hits,
            binding_cache_misses: metrics.binding_cache_misses,
        }
    })
}

fn resolve_cached_binding(context: &mut EvalContext, name: &str) -> Option<abi::RuntimeBinding> {
    if let Some(binding) = context.binding_cache.get(name).copied() {
        context.record_binding_cache_hit();
        return Some(binding);
    }

    let resolved = abi::resolve_runtime_identifier_binding(name);
    context.record_binding_cache_miss();

    if let Some(binding) = resolved {
        context.binding_cache.insert(name.to_string(), binding);
        Some(binding)
    } else {
        None
    }
}

fn bind_symbol(context: &mut EvalContext, name: &str, value: RuntimeValue, mutable: bool) {
    let handle = abi::bind_runtime_identifier_value(name, value, mutable);
    context
        .binding_cache
        .insert(name.to_string(), abi::RuntimeBinding { handle, mutable });
}

fn read_symbol_value(context: &mut EvalContext, name: &str) -> RuntimeValue {
    if name == "undefined" {
        return RuntimeValue::Undefined;
    }

    context.record_identifier_read();

    let Some(binding) = resolve_cached_binding(context, name) else {
        return RuntimeValue::Undefined;
    };
    abi::read_runtime_value(binding.handle)
}

fn write_symbol_value(context: &mut EvalContext, name: &str, value: RuntimeValue) {
    context.record_identifier_write();

    if let Some(binding) = resolve_cached_binding(context, name) {
        if !binding.mutable {
            return;
        }
        if abi::write_runtime_value_handle(binding.handle, value.clone()) {
            return;
        }
    }

    abi::write_runtime_identifier_value(name, value);
    if let Some(refreshed) = abi::resolve_runtime_identifier_binding(name) {
        context.binding_cache.insert(name.to_string(), refreshed);
    }
}

#[derive(Debug, Clone)]
enum Flow {
    Normal(RuntimeValue),
    Break,
    Continue,
    Return(RuntimeValue),
}

pub(crate) fn eval_expression_text(expr: &str) -> RuntimeValue {
    let started = Instant::now();
    let mut context = EvalContext::new();
    let wrapped = format!("{expr};");
    let value = if let Some(statements) = parse_statements(&wrapped) {
        if let Some(Stmt::Expr(expr_stmt)) = statements.first() {
            eval_expr(&expr_stmt.expr, &mut context)
        } else {
            RuntimeValue::Undefined
        }
    } else {
        RuntimeValue::Undefined
    };
    let elapsed = started.elapsed().as_nanos();
    with_metrics(|metrics| {
        metrics.eval_expr_calls = metrics.eval_expr_calls.saturating_add(1);
        metrics.eval_expr_nanos = metrics.eval_expr_nanos.saturating_add(elapsed);
    });
    context.flush_metrics();
    value
}

pub(crate) fn eval_statement_text(stmt: &str) -> RuntimeValue {
    let started = Instant::now();
    let mut context = EvalContext::new();
    let result = if let Some(statements) = parse_statements(stmt) {
        match eval_statements(&statements, &mut context) {
            Flow::Normal(value) | Flow::Return(value) => value,
            Flow::Break | Flow::Continue => RuntimeValue::Undefined,
        }
    } else {
        RuntimeValue::Undefined
    };
    let elapsed = started.elapsed().as_nanos();
    with_metrics(|metrics| {
        metrics.eval_stmt_calls = metrics.eval_stmt_calls.saturating_add(1);
        metrics.eval_stmt_nanos = metrics.eval_stmt_nanos.saturating_add(elapsed);
    });
    context.flush_metrics();
    result
}

fn parse_statements(source_text: &str) -> Option<Vec<Stmt>> {
    let started = Instant::now();
    let cm: Lrc<SourceMap> = Default::default();
    let source = cm.new_source_file(FileName::Anon.into(), source_text.to_string());
    let mut parser = Parser::new(
        Syntax::Typescript(TsSyntax::default()),
        StringInput::from(&*source),
        None,
    );
    let parsed = parser.parse_script().ok().map(|script| script.body);
    let elapsed = started.elapsed().as_nanos();
    with_metrics(|metrics| {
        metrics.parse_calls = metrics.parse_calls.saturating_add(1);
        metrics.parse_nanos = metrics.parse_nanos.saturating_add(elapsed);
    });
    parsed
}

fn eval_statements(statements: &[Stmt], context: &mut EvalContext) -> Flow {
    let mut last = RuntimeValue::Undefined;
    for statement in statements {
        match eval_stmt(statement, context) {
            Flow::Normal(value) => last = value,
            Flow::Break => return Flow::Break,
            Flow::Continue => return Flow::Continue,
            Flow::Return(value) => return Flow::Return(value),
        }
    }
    Flow::Normal(last)
}

fn eval_stmt(statement: &Stmt, context: &mut EvalContext) -> Flow {
    match statement {
        Stmt::Decl(Decl::Var(var_decl)) => {
            for decl in &var_decl.decls {
                let Some(name) = pat_ident_name(&decl.name) else {
                    continue;
                };
                let value = decl
                    .init
                    .as_deref()
                    .map(|expr| eval_expr(expr, context))
                    .unwrap_or(RuntimeValue::Undefined);
                let mutable = var_decl.kind != VarDeclKind::Const;
                bind_symbol(context, name, value, mutable);
            }
            Flow::Normal(RuntimeValue::Undefined)
        }
        Stmt::Expr(expr_stmt) => Flow::Normal(eval_expr(&expr_stmt.expr, context)),
        Stmt::Block(block_stmt) => eval_statements(&block_stmt.stmts, context),
        Stmt::If(if_stmt) => {
            let condition = eval_expr(&if_stmt.test, context);
            if condition.truthy() {
                eval_stmt(&if_stmt.cons, context)
            } else if let Some(alt) = &if_stmt.alt {
                eval_stmt(alt, context)
            } else {
                Flow::Normal(RuntimeValue::Undefined)
            }
        }
        Stmt::While(while_stmt) => {
            let mut last = RuntimeValue::Undefined;
            loop {
                if !eval_expr(&while_stmt.test, context).truthy() {
                    break;
                }
                match eval_stmt(&while_stmt.body, context) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => continue,
                    Flow::Return(value) => return Flow::Return(value),
                }
            }
            Flow::Normal(last)
        }
        Stmt::DoWhile(do_while_stmt) => {
            let mut last = RuntimeValue::Undefined;
            loop {
                match eval_stmt(&do_while_stmt.body, context) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => {}
                    Flow::Return(value) => return Flow::Return(value),
                }
                if !eval_expr(&do_while_stmt.test, context).truthy() {
                    break;
                }
            }
            Flow::Normal(last)
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                match init {
                    VarDeclOrExpr::VarDecl(var_decl) => {
                        let _ = eval_stmt(&Stmt::Decl(Decl::Var(var_decl.clone())), context);
                    }
                    VarDeclOrExpr::Expr(expr) => {
                        let _ = eval_expr(expr, context);
                    }
                }
            }

            let mut last = RuntimeValue::Undefined;
            loop {
                if let Some(test) = &for_stmt.test {
                    if !eval_expr(test, context).truthy() {
                        break;
                    }
                }

                match eval_stmt(&for_stmt.body, context) {
                    Flow::Normal(value) => last = value,
                    Flow::Break => break,
                    Flow::Continue => {}
                    Flow::Return(value) => return Flow::Return(value),
                }

                if let Some(update) = &for_stmt.update {
                    let _ = eval_expr(update, context);
                }
            }
            Flow::Normal(last)
        }
        Stmt::Switch(switch_stmt) => eval_switch_stmt(switch_stmt, context),
        Stmt::Break(_) => Flow::Break,
        Stmt::Continue(_) => Flow::Continue,
        Stmt::Return(return_stmt) => {
            let value = return_stmt
                .arg
                .as_deref()
                .map(|expr| eval_expr(expr, context))
                .unwrap_or(RuntimeValue::Undefined);
            Flow::Return(value)
        }
        _ => Flow::Normal(RuntimeValue::Undefined),
    }
}

fn eval_switch_stmt(switch_stmt: &SwitchStmt, context: &mut EvalContext) -> Flow {
    let discriminant = eval_expr(&switch_stmt.discriminant, context);
    let mut default_index = None;
    let mut start_index = None;

    for (index, case) in switch_stmt.cases.iter().enumerate() {
        if case.test.is_none() {
            default_index = Some(index);
            continue;
        }
        if start_index.is_none() {
            let Some(test) = case.test.as_deref() else {
                continue;
            };
            let case_value = eval_expr(test, context);
            if strict_equal(&discriminant, &case_value) {
                start_index = Some(index);
            }
        }
    }

    let Some(mut index) = start_index.or(default_index) else {
        return Flow::Normal(RuntimeValue::Undefined);
    };

    let mut last = RuntimeValue::Undefined;
    while index < switch_stmt.cases.len() {
        for statement in &switch_stmt.cases[index].cons {
            match eval_stmt(statement, context) {
                Flow::Normal(value) => last = value,
                Flow::Break => return Flow::Normal(last),
                Flow::Continue => return Flow::Continue,
                Flow::Return(value) => return Flow::Return(value),
            }
        }
        index += 1;
    }

    Flow::Normal(last)
}

fn eval_expr(expr: &Expr, context: &mut EvalContext) -> RuntimeValue {
    match expr {
        Expr::Lit(lit) => match lit {
            Lit::Num(number) => RuntimeValue::Number(number.value),
            Lit::Str(string) => RuntimeValue::String(string.value.to_string_lossy().into_owned()),
            Lit::Bool(boolean) => RuntimeValue::Bool(boolean.value),
            Lit::Null(_) => RuntimeValue::Null,
            _ => RuntimeValue::Undefined,
        },
        Expr::Ident(ident) => read_symbol_value(context, ident.sym.as_ref()),
        Expr::Paren(paren) => eval_expr(&paren.expr, context),
        Expr::Seq(sequence) => {
            let mut last = RuntimeValue::Undefined;
            for item in &sequence.exprs {
                last = eval_expr(item, context);
            }
            last
        }
        Expr::Bin(binary) => eval_bin_expr(binary, context),
        Expr::Unary(unary) => eval_unary_expr(unary, context),
        Expr::Assign(assign) => eval_assign_expr(assign, context),
        Expr::Update(update) => eval_update_expr(update, context),
        Expr::Call(call) => eval_call_expr(call, context),
        Expr::Member(member) => eval_member_expr(member, context),
        Expr::Array(array) => {
            let items: Vec<RuntimeValue> = array
                .elems
                .iter()
                .map(|elem| match elem {
                    Some(spread) => eval_expr(&spread.expr, context),
                    None => RuntimeValue::Undefined,
                })
                .collect();
            RuntimeValue::Array(items)
        }
        _ => RuntimeValue::Undefined,
    }
}

fn eval_member_expr(member: &MemberExpr, context: &mut EvalContext) -> RuntimeValue {
    let obj = eval_expr(&member.obj, context);
    let key = match &member.prop {
        MemberProp::Ident(ident) => ident.sym.to_string(),
        MemberProp::Computed(computed) => {
            let val = eval_expr(&computed.expr, context);
            match val {
                RuntimeValue::Number(n) if n.fract() == 0.0 => format!("{}", n as i64),
                RuntimeValue::String(s) => s,
                other => other.to_runtime_string(),
            }
        }
        _ => return RuntimeValue::Undefined,
    };
    obj.get_property(&key).unwrap_or(RuntimeValue::Undefined)
}

fn eval_unary_expr(unary: &UnaryExpr, context: &mut EvalContext) -> RuntimeValue {
    let value = eval_expr(&unary.arg, context);
    match unary.op {
        UnaryOp::Minus => RuntimeValue::Number(-value.to_number()),
        UnaryOp::Plus => RuntimeValue::Number(value.to_number()),
        UnaryOp::Bang => RuntimeValue::Bool(!value.truthy()),
        _ => RuntimeValue::Undefined,
    }
}

fn eval_bin_expr(binary: &BinExpr, context: &mut EvalContext) -> RuntimeValue {
    if matches!(binary.op, BinaryOp::LogicalAnd) {
        let lhs = eval_expr(&binary.left, context);
        if lhs.truthy() {
            return eval_expr(&binary.right, context);
        }
        return lhs;
    }

    if matches!(binary.op, BinaryOp::LogicalOr) {
        let lhs = eval_expr(&binary.left, context);
        if lhs.truthy() {
            return lhs;
        }
        return eval_expr(&binary.right, context);
    }

    let lhs = eval_expr(&binary.left, context);
    let rhs = eval_expr(&binary.right, context);

    match binary.op {
        BinaryOp::Add => {
            if lhs.is_string_like() || rhs.is_string_like() {
                RuntimeValue::String(format!(
                    "{}{}",
                    lhs.to_runtime_string(),
                    rhs.to_runtime_string()
                ))
            } else {
                RuntimeValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        BinaryOp::Sub => RuntimeValue::Number(lhs.to_number() - rhs.to_number()),
        BinaryOp::Mul => RuntimeValue::Number(lhs.to_number() * rhs.to_number()),
        BinaryOp::Div => RuntimeValue::Number(lhs.to_number() / rhs.to_number()),
        BinaryOp::Mod => RuntimeValue::Number(lhs.to_number() % rhs.to_number()),
        BinaryOp::Lt => RuntimeValue::Bool(lhs.to_number() < rhs.to_number()),
        BinaryOp::LtEq => RuntimeValue::Bool(lhs.to_number() <= rhs.to_number()),
        BinaryOp::Gt => RuntimeValue::Bool(lhs.to_number() > rhs.to_number()),
        BinaryOp::GtEq => RuntimeValue::Bool(lhs.to_number() >= rhs.to_number()),
        BinaryOp::EqEqEq | BinaryOp::EqEq => RuntimeValue::Bool(strict_equal(&lhs, &rhs)),
        BinaryOp::NotEqEq | BinaryOp::NotEq => RuntimeValue::Bool(!strict_equal(&lhs, &rhs)),
        _ => RuntimeValue::Undefined,
    }
}

fn eval_assign_expr(assign: &AssignExpr, context: &mut EvalContext) -> RuntimeValue {
    let Some(name) = assign_target_name(&assign.left) else {
        return RuntimeValue::Undefined;
    };

    let rhs = eval_expr(&assign.right, context);
    let next = match assign.op {
        AssignOp::Assign => rhs,
        AssignOp::AddAssign => {
            let lhs = read_symbol_value(context, name);
            if lhs.is_string_like() || rhs.is_string_like() {
                RuntimeValue::String(format!(
                    "{}{}",
                    lhs.to_runtime_string(),
                    rhs.to_runtime_string()
                ))
            } else {
                RuntimeValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        AssignOp::SubAssign => {
            let lhs = read_symbol_value(context, name);
            RuntimeValue::Number(lhs.to_number() - rhs.to_number())
        }
        AssignOp::MulAssign => {
            let lhs = read_symbol_value(context, name);
            RuntimeValue::Number(lhs.to_number() * rhs.to_number())
        }
        AssignOp::DivAssign => {
            let lhs = read_symbol_value(context, name);
            RuntimeValue::Number(lhs.to_number() / rhs.to_number())
        }
        AssignOp::ModAssign => {
            let lhs = read_symbol_value(context, name);
            RuntimeValue::Number(lhs.to_number() % rhs.to_number())
        }
        _ => RuntimeValue::Undefined,
    };

    write_symbol_value(context, name, next.clone());
    next
}

fn eval_update_expr(update: &UpdateExpr, context: &mut EvalContext) -> RuntimeValue {
    let Expr::Ident(ident) = update.arg.as_ref() else {
        return RuntimeValue::Undefined;
    };

    let name = ident.sym.as_ref();
    let current = read_symbol_value(context, name).to_number();
    let next = match update.op {
        UpdateOp::PlusPlus => current + 1.0,
        UpdateOp::MinusMinus => current - 1.0,
    };
    write_symbol_value(context, name, RuntimeValue::Number(next));

    if update.prefix {
        RuntimeValue::Number(next)
    } else {
        RuntimeValue::Number(current)
    }
}

fn eval_call_expr(call: &CallExpr, context: &mut EvalContext) -> RuntimeValue {
    let Some(callee) = callee_name(&call.callee) else {
        return RuntimeValue::Undefined;
    };

    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        args.push(eval_expr(&arg.expr, context));
    }
    context.record_call_dispatch();

    let Some(outcome) = super::dispatch_runtime_call(callee.as_str(), &args) else {
        return RuntimeValue::Undefined;
    };

    match outcome {
        DispatchOutcome::Value(value) => value,
        DispatchOutcome::Emit(message) => {
            if callee == "io.stderr_write" {
                eprint!("{message}");
            } else if callee == "io.stdout_write" {
                print!("{message}");
            } else {
                println!("{message}");
            }
            RuntimeValue::Undefined
        }
        DispatchOutcome::Panic(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn strict_equal(lhs: &RuntimeValue, rhs: &RuntimeValue) -> bool {
    lhs == rhs
}

fn pat_ident_name(pattern: &Pat) -> Option<&str> {
    match pattern {
        Pat::Ident(ident) => Some(ident.id.sym.as_ref()),
        _ => None,
    }
}

fn assign_target_name(target: &AssignTarget) -> Option<&str> {
    match target {
        AssignTarget::Simple(simple) => match simple {
            SimpleAssignTarget::Ident(ident) => Some(ident.id.sym.as_ref()),
            _ => None,
        },
        _ => None,
    }
}

fn callee_name(callee: &Callee) -> Option<String> {
    match callee {
        Callee::Expr(expr) => expr_name(expr),
        _ => None,
    }
}

fn expr_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Member(member) => {
            let object = expr_name(&member.obj)?;
            let property = match &member.prop {
                MemberProp::Ident(ident) => ident.sym.to_string(),
                _ => return None,
            };
            Some(format!("{object}.{property}"))
        }
        _ => None,
    }
}
