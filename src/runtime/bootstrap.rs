use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::compile_options::CompileOptions;
use crate::module_system::{ModuleGraph, SourceModule};
use crate::runtime::bootstrap_lang::{JsValue, RuntimeContext, evaluate_expression};
use crate::runtime::bootstrap_utils::{
    brace_delta, is_identifier_like, normalize, split_top_level, split_top_level_once,
    strip_comment,
};
use crate::runtime::namespaces::{self as runtime_namespaces, DispatchOutcome, NamespaceUsage};

const BOOTSTRAP_MAGIC: &[u8] = b"RTS_BOOTSTRAP_BIN\0";

const FLAG_TRACE_DATA: u8 = 0b0000_0001;

const OP_WRITE_LINE: u8 = 1;
const MAX_CALL_DEPTH: usize = 32;

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub lines_emitted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapProgram {
    pub ops: Vec<BootstrapOp>,
    pub traces: Vec<Option<SourceTrace>>,
}

impl BootstrapProgram {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let include_trace =
            self.traces.iter().any(|trace| trace.is_some()) && self.traces.len() == self.ops.len();
        let flags = if include_trace { FLAG_TRACE_DATA } else { 0 };
        let payload_key = derive_payload_key(self);

        out.extend_from_slice(BOOTSTRAP_MAGIC);
        out.push(flags);
        out.extend_from_slice(&payload_key.to_le_bytes());
        out.extend_from_slice(&(self.ops.len() as u32).to_le_bytes());

        for (index, op) in self.ops.iter().enumerate() {
            match op {
                BootstrapOp::WriteLine(line) => {
                    out.push(OP_WRITE_LINE);
                    let encoded = obfuscate(line.as_bytes(), payload_key, index as u32, 0);
                    out.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
                    out.extend_from_slice(&encoded);

                    if include_trace {
                        match self.traces.get(index).and_then(|trace| trace.clone()) {
                            Some(trace) => {
                                out.push(1);

                                let module = obfuscate(
                                    trace.module.as_bytes(),
                                    payload_key,
                                    index as u32,
                                    1,
                                );
                                out.extend_from_slice(&(module.len() as u32).to_le_bytes());
                                out.extend_from_slice(&module);
                                out.extend_from_slice(&trace.line.to_le_bytes());
                                out.extend_from_slice(&trace.column.to_le_bytes());
                            }
                            None => out.push(0),
                        }
                    }
                }
            }
        }

        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < BOOTSTRAP_MAGIC.len() + std::mem::size_of::<u32>() {
            bail!("bootstrap payload is too short");
        }

        if !bytes.starts_with(BOOTSTRAP_MAGIC) {
            bail!("invalid bootstrap payload magic");
        }

        decode_payload(bytes)
    }
}

#[derive(Debug, Clone)]
pub enum BootstrapOp {
    WriteLine(String),
}

#[derive(Debug, Clone)]
pub struct SourceTrace {
    pub module: String,
    pub line: u32,
    pub column: u32,
}

impl SourceTrace {
    fn new(module: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            module: module.into(),
            line,
            column,
        }
    }
}

pub fn compile_graph(graph: &ModuleGraph, options: CompileOptions) -> Result<BootstrapProgram> {
    let Some(entry) = graph.entry() else {
        return Ok(BootstrapProgram::default());
    };

    let capture_trace = options.include_trace_data();
    let namespace_usage =
        NamespaceUsage::from_sources(graph.modules().map(|module| module.source.as_str()));

    let mut functions = BTreeMap::<String, FunctionDef>::new();
    for module in graph.modules() {
        collect_functions(
            &module.source,
            &trace_module_name(module),
            &mut functions,
            capture_trace,
        );
    }

    let top_level =
        collect_top_level_statements(&entry.source, &trace_module_name(entry), capture_trace);

    let (ops, traces) =
        ExecutionEngine::new(&functions, capture_trace, namespace_usage).run(&top_level);

    Ok(BootstrapProgram { ops, traces })
}

pub fn compile_source(entry_source: &str, options: CompileOptions) -> BootstrapProgram {
    let capture_trace = options.include_trace_data();
    let namespace_usage = NamespaceUsage::from_sources(std::iter::once(entry_source));

    let mut functions = BTreeMap::<String, FunctionDef>::new();
    collect_functions(entry_source, "<entry>", &mut functions, capture_trace);

    let top_level = collect_top_level_statements(entry_source, "<entry>", capture_trace);
    let (ops, traces) =
        ExecutionEngine::new(&functions, capture_trace, namespace_usage).run(&top_level);

    BootstrapProgram { ops, traces }
}

pub fn execute(program: &BootstrapProgram) -> RunReport {
    let mut report = RunReport::default();
    let emit_trace = matches!(std::env::var("RTS_DEBUG_TRACE"), Ok(value) if is_truthy_env(&value));

    for (index, op) in program.ops.iter().enumerate() {
        match op {
            BootstrapOp::WriteLine(line) => {
                println!("{line}");
                report.lines_emitted += 1;
            }
        }

        if emit_trace {
            if let Some(Some(trace)) = program.traces.get(index) {
                eprintln!(
                    "[rts:trace] {}:{}:{}",
                    trace.module, trace.line, trace.column
                );
            }
        }
    }

    report
}

#[derive(Debug, Clone)]
struct FunctionDef {
    parameters: Vec<String>,
    body: Vec<SourceLine>,
}

#[derive(Debug, Clone)]
struct SourceLine {
    text: String,
    trace: Option<SourceTrace>,
}

impl SourceLine {
    fn new(text: impl Into<String>, trace: Option<SourceTrace>) -> Self {
        Self {
            text: text.into(),
            trace,
        }
    }
}

struct ExecutionEngine<'a> {
    functions: &'a BTreeMap<String, FunctionDef>,
    capture_trace: bool,
    namespace_usage: NamespaceUsage,
    ops: Vec<BootstrapOp>,
    traces: Vec<Option<SourceTrace>>,
    scopes: Vec<BTreeMap<String, JsValue>>,
    current_trace: Option<SourceTrace>,
    call_depth: usize,
}

impl<'a> ExecutionEngine<'a> {
    fn new(
        functions: &'a BTreeMap<String, FunctionDef>,
        capture_trace: bool,
        namespace_usage: NamespaceUsage,
    ) -> Self {
        Self {
            functions,
            capture_trace,
            namespace_usage,
            ops: Vec::new(),
            traces: Vec::new(),
            scopes: vec![BTreeMap::new()],
            current_trace: None,
            call_depth: 0,
        }
    }

    fn run(mut self, statements: &[SourceLine]) -> (Vec<BootstrapOp>, Vec<Option<SourceTrace>>) {
        let _ = self.execute_statements(statements);
        (self.ops, self.traces)
    }

    fn execute_statements(&mut self, statements: &[SourceLine]) -> Option<JsValue> {
        for statement in statements {
            let line = normalize(&statement.text);
            if line.is_empty() || line == "{" || line == "}" {
                continue;
            }

            let previous_trace = self.current_trace.clone();
            self.current_trace = statement.trace.clone();

            if let Some(return_expression) = parse_return_statement(line) {
                let value = return_expression
                    .map(|expr| self.eval_expression_safe(expr))
                    .unwrap_or(JsValue::Undefined);
                self.current_trace = previous_trace;
                return Some(value);
            }

            if self.handle_variable_declaration(line) {
                self.current_trace = previous_trace;
                continue;
            }

            let expression = line.trim_end_matches(';').trim();
            if !expression.is_empty() {
                let _ = self.eval_expression_safe(expression);
            }

            self.current_trace = previous_trace;
        }

        None
    }

    fn handle_variable_declaration(&mut self, line: &str) -> bool {
        let Some(rest) = strip_variable_keyword(line) else {
            return false;
        };

        let declarators = rest.trim_end_matches(';').trim();
        if declarators.is_empty() {
            return true;
        }

        for part in split_top_level(declarators, ',') {
            let declarator = part.trim();
            if declarator.is_empty() {
                continue;
            }

            let (binding, initializer) =
                if let Some((left, right)) = split_top_level_once(declarator, '=') {
                    (left.trim(), Some(right.trim()))
                } else {
                    (declarator, None)
                };

            let Some(name) = normalize_binding_name(binding) else {
                continue;
            };

            let value = initializer
                .filter(|expr| !expr.is_empty())
                .map(|expr| self.eval_expression_safe(expr))
                .unwrap_or(JsValue::Undefined);

            self.define_variable(name, value);
        }

        true
    }

    fn eval_expression_safe(&mut self, expression: &str) -> JsValue {
        let expr = expression.trim();
        if expr.is_empty() {
            return JsValue::Undefined;
        }

        match evaluate_expression(expr, self) {
            Ok(value) => value,
            Err(_) => fallback_literal(expr),
        }
    }

    fn define_variable(&mut self, name: String, value: JsValue) {
        if self.scopes.is_empty() {
            self.scopes.push(BTreeMap::new());
        }

        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    fn read_variable(&self, name: &str) -> Option<JsValue> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn call_user_function(&mut self, name: &str, args: Vec<JsValue>) -> Result<JsValue> {
        let Some(function) = self.functions.get(name).cloned() else {
            return Ok(JsValue::Undefined);
        };

        if self.call_depth >= MAX_CALL_DEPTH {
            return Ok(JsValue::Undefined);
        }

        self.call_depth += 1;
        self.scopes.push(BTreeMap::new());

        for (index, parameter) in function.parameters.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(JsValue::Undefined);
            self.define_variable(parameter.clone(), value);
        }

        let result = self
            .execute_statements(&function.body)
            .unwrap_or(JsValue::Undefined);

        let _ = self.scopes.pop();
        self.call_depth -= 1;

        Ok(result)
    }

    fn emit_write_line(&mut self, value: String) {
        self.ops.push(BootstrapOp::WriteLine(value));
        self.traces.push(if self.capture_trace {
            self.current_trace.clone()
        } else {
            None
        });
    }
}

impl RuntimeContext for ExecutionEngine<'_> {
    fn read_identifier(&self, name: &str) -> Option<JsValue> {
        self.read_variable(name)
            .or_else(|| runtime_namespaces::namespace_object(name, &self.namespace_usage))
    }

    fn call_function(&mut self, callee: &str, args: Vec<JsValue>) -> Result<JsValue> {
        if self.namespace_usage.is_builtin_callee(callee)
            && !self.namespace_usage.is_function_enabled(callee)
        {
            return Ok(JsValue::Undefined);
        }

        if let Some(outcome) = runtime_namespaces::dispatch(callee, &args) {
            return match outcome {
                DispatchOutcome::Value(value) => Ok(value),
                DispatchOutcome::Emit(line) => {
                    self.emit_write_line(line);
                    Ok(JsValue::Undefined)
                }
                DispatchOutcome::Panic(message) => bail!(message),
            };
        }

        match callee {
            "Number" => Ok(JsValue::Number(
                args.first()
                    .cloned()
                    .unwrap_or(JsValue::Undefined)
                    .to_number(),
            )),
            "String" => Ok(JsValue::String(
                args.first()
                    .cloned()
                    .unwrap_or(JsValue::Undefined)
                    .to_js_string(),
            )),
            "Boolean" => Ok(JsValue::Bool(
                args.first().cloned().unwrap_or(JsValue::Undefined).truthy(),
            )),
            _ => self.call_user_function(callee, args),
        }
    }
}

fn collect_top_level_statements(
    source: &str,
    module_name: &str,
    capture_trace: bool,
) -> Vec<SourceLine> {
    let mut statements = Vec::new();
    let mut depth = 0i32;

    for (line_index, raw_line) in source.lines().enumerate() {
        let line = strip_comment(raw_line);
        let trimmed = line.trim();

        if trimmed.is_empty() {
            depth += brace_delta(trimmed);
            continue;
        }

        if depth == 0 && !trimmed.starts_with("import ") && !trimmed.starts_with("export ") {
            if !trimmed.starts_with("function ") && !trimmed.starts_with("class ") {
                let trace =
                    capture_trace.then(|| SourceTrace::new(module_name, line_index as u32 + 1, 1));
                statements.push(SourceLine::new(trimmed, trace));
            }
        }

        depth += brace_delta(trimmed);
    }

    statements
}

fn collect_functions(
    source: &str,
    module_name: &str,
    functions: &mut BTreeMap<String, FunctionDef>,
    capture_trace: bool,
) {
    let mut in_function = false;
    let mut function_depth = 0i32;
    let mut function_name = String::new();
    let mut function_parameters = Vec::<String>::new();
    let mut body = Vec::<SourceLine>::new();

    for (line_index, raw_line) in source.lines().enumerate() {
        let line = strip_comment(raw_line).trim().to_string();

        if !in_function {
            if let Some(signature) = parse_function_decl_signature(&line) {
                in_function = true;
                function_name = signature.name;
                function_parameters = signature.parameters;
                function_depth = brace_delta(&line);
                body.clear();
                continue;
            }
            continue;
        }

        let trace = capture_trace.then(|| SourceTrace::new(module_name, line_index as u32 + 1, 1));
        body.push(SourceLine::new(line.clone(), trace));
        function_depth += brace_delta(&line);

        if function_depth <= 0 {
            let cleaned = strip_trailing_braces(&body);
            functions
                .entry(function_name.clone())
                .or_insert(FunctionDef {
                    parameters: function_parameters.clone(),
                    body: cleaned,
                });

            in_function = false;
            function_depth = 0;
            function_name.clear();
            function_parameters.clear();
            body.clear();
        }
    }
}

struct FunctionDeclSignature {
    name: String,
    parameters: Vec<String>,
}

fn parse_function_decl_signature(line: &str) -> Option<FunctionDeclSignature> {
    let trimmed = line.strip_prefix("export ").unwrap_or(line).trim();
    let decl = trimmed.strip_prefix("function ")?;
    let open = decl.find('(')?;
    let close = decl[open + 1..].find(')')? + open + 1;
    let name = decl[..open].trim().to_string();
    let raw_parameters = &decl[open + 1..close];
    let parameters = parse_parameter_names(raw_parameters);

    if name.is_empty() {
        return None;
    }

    Some(FunctionDeclSignature { name, parameters })
}

fn parse_parameter_names(raw: &str) -> Vec<String> {
    split_top_level(raw, ',')
        .into_iter()
        .filter_map(|part| {
            let mut text = part.trim();
            if text.is_empty() {
                return None;
            }

            if let Some(rest) = text.strip_prefix("...") {
                text = rest.trim_start();
            }

            let text = split_top_level_once(text, ':')
                .map(|(left, _)| left.trim())
                .unwrap_or(text);
            let text = split_top_level_once(text, '=')
                .map(|(left, _)| left.trim())
                .unwrap_or(text)
                .trim();

            if is_identifier_like(text) {
                Some(text.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn parse_return_statement(line: &str) -> Option<Option<&str>> {
    let stripped = line.strip_prefix("return")?;
    if !stripped.is_empty()
        && !stripped.starts_with(char::is_whitespace)
        && !stripped.starts_with(';')
    {
        return None;
    }

    let expression = stripped.trim().trim_end_matches(';').trim();
    if expression.is_empty() {
        Some(None)
    } else {
        Some(Some(expression))
    }
}

fn strip_variable_keyword(line: &str) -> Option<&str> {
    for keyword in ["const", "let", "var"] {
        if let Some(rest) = line.strip_prefix(keyword) {
            if rest.is_empty() {
                return Some(rest);
            }

            if rest.starts_with(char::is_whitespace) {
                return Some(rest.trim_start());
            }
        }
    }

    None
}

fn normalize_binding_name(raw: &str) -> Option<String> {
    let candidate = split_top_level_once(raw.trim(), ':')
        .map(|(left, _)| left.trim())
        .unwrap_or(raw)
        .trim();

    if is_identifier_like(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn fallback_literal(expr: &str) -> JsValue {
    let trimmed = expr.trim();

    if let Some(string) = strip_quotes(trimmed) {
        return JsValue::String(string.to_string());
    }

    if let Ok(number) = trimmed.parse::<f64>() {
        return JsValue::Number(number);
    }

    match trimmed {
        "true" => JsValue::Bool(true),
        "false" => JsValue::Bool(false),
        "null" => JsValue::Null,
        "undefined" => JsValue::Undefined,
        _ => JsValue::String(trimmed.to_string()),
    }
}

fn strip_quotes(value: &str) -> Option<&str> {
    if value.len() < 2 {
        return None;
    }

    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
        || (value.starts_with('`') && value.ends_with('`'))
    {
        Some(&value[1..value.len() - 1])
    } else {
        None
    }
}

fn strip_trailing_braces(lines: &[SourceLine]) -> Vec<SourceLine> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.text.trim();
            if trimmed == "}" {
                None
            } else {
                Some(SourceLine::new(trimmed, line.trace.clone()))
            }
        })
        .collect()
}

fn trace_module_name(module: &SourceModule) -> String {
    module.path.display().to_string()
}

fn decode_payload(bytes: &[u8]) -> Result<BootstrapProgram> {
    let mut cursor = BOOTSTRAP_MAGIC.len();

    let flags = *bytes
        .get(cursor)
        .ok_or_else(|| anyhow::anyhow!("bootstrap payload is missing flags"))?;
    cursor += 1;

    let payload_key = read_u32(bytes, &mut cursor)?;
    let op_count = read_u32(bytes, &mut cursor)? as usize;
    let include_trace = (flags & FLAG_TRACE_DATA) != 0;

    let mut ops = Vec::with_capacity(op_count);
    let mut traces = Vec::with_capacity(op_count);

    for index in 0..op_count {
        let opcode = *bytes
            .get(cursor)
            .ok_or_else(|| anyhow::anyhow!("unexpected end of bootstrap payload"))?;
        cursor += 1;

        match opcode {
            OP_WRITE_LINE => {
                let encoded_line = read_sized_blob(bytes, &mut cursor)?;
                let line =
                    String::from_utf8(deobfuscate(&encoded_line, payload_key, index as u32, 0))
                        .context("bootstrap line is not valid UTF-8")?;
                ops.push(BootstrapOp::WriteLine(line));

                if include_trace {
                    let has_trace = *bytes.get(cursor).ok_or_else(|| {
                        anyhow::anyhow!("unexpected end of bootstrap trace metadata")
                    })?;
                    cursor += 1;

                    if has_trace == 1 {
                        let encoded_module = read_sized_blob(bytes, &mut cursor)?;
                        let module = String::from_utf8(deobfuscate(
                            &encoded_module,
                            payload_key,
                            index as u32,
                            1,
                        ))
                        .context("bootstrap trace module is not valid UTF-8")?;
                        let line = read_u32(bytes, &mut cursor)?;
                        let column = read_u32(bytes, &mut cursor)?;
                        traces.push(Some(SourceTrace::new(module, line, column)));
                    } else {
                        traces.push(None);
                    }
                } else {
                    traces.push(None);
                }
            }
            other => bail!("unknown bootstrap opcode {}", other),
        }
    }

    Ok(BootstrapProgram { ops, traces })
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor
        .checked_add(std::mem::size_of::<u32>())
        .ok_or_else(|| anyhow::anyhow!("bootstrap cursor overflow"))?;

    if end > bytes.len() {
        bail!("unexpected end of bootstrap payload");
    }

    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[*cursor..end]);
    *cursor = end;

    Ok(u32::from_le_bytes(raw))
}

fn read_sized_blob(bytes: &[u8], cursor: &mut usize) -> Result<Vec<u8>> {
    let len = read_u32(bytes, cursor)? as usize;
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| anyhow::anyhow!("bootstrap payload length overflow"))?;

    if end > bytes.len() {
        bail!("invalid bootstrap payload length");
    }

    let blob = bytes[*cursor..end].to_vec();
    *cursor = end;
    Ok(blob)
}

fn derive_payload_key(program: &BootstrapProgram) -> u32 {
    let mut key = 0xA531_42D9u32;
    for (index, op) in program.ops.iter().enumerate() {
        match op {
            BootstrapOp::WriteLine(line) => {
                key ^= (line.len() as u32).wrapping_mul((index as u32).wrapping_add(1));
                key = key.rotate_left(5).wrapping_mul(0x9E37_79B1);
                for byte in line.as_bytes().iter().take(32) {
                    key ^= u32::from(*byte);
                    key = key.wrapping_mul(0x0100_0193);
                }
            }
        }
    }

    if key == 0 { 0x3F4A_BC19 } else { key }
}

fn obfuscate(input: &[u8], base_key: u32, op_index: u32, lane: u32) -> Vec<u8> {
    let mut state = base_key
        .wrapping_add(op_index.wrapping_mul(0x045D_9F3B))
        .wrapping_add(lane.wrapping_mul(0x9E37_79B9));
    let mut out = Vec::with_capacity(input.len());

    for (idx, byte) in input.iter().enumerate() {
        state = state
            .rotate_left(7)
            .wrapping_add(0xA5A5_9651)
            .wrapping_add((idx as u32).wrapping_mul(0x27D4_EB2D));
        out.push(*byte ^ (state as u8));
    }

    out
}

fn deobfuscate(input: &[u8], base_key: u32, op_index: u32, lane: u32) -> Vec<u8> {
    obfuscate(input, base_key, op_index, lane)
}

fn is_truthy_env(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_respects_numeric_addition_before_string_concat() {
        let source = r#"
io.print(1+1 + " hello from package console");
io.print((1+1) + " hello from package console");
"#;

        let program = compile_source(source, CompileOptions::default());
        let lines = program
            .ops
            .iter()
            .map(|op| match op {
                BootstrapOp::WriteLine(line) => line.clone(),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            lines,
            vec![
                "2 hello from package console".to_string(),
                "2 hello from package console".to_string()
            ]
        );
    }

    #[test]
    fn supports_variables_and_function_returns() {
        let source = r#"
const base = 10;
function calc(a: number, b: number) {
  const mixed = a * b + base;
  return mixed;
}
io.print("result=" + calc(2, 3));
"#;

        let program = compile_source(source, CompileOptions::default());
        let lines = program
            .ops
            .iter()
            .map(|op| match op {
                BootstrapOp::WriteLine(line) => line.clone(),
            })
            .collect::<Vec<_>>();

        assert_eq!(lines, vec!["result=16".to_string()]);
    }

    #[test]
    fn supports_std_style_fs_and_io_result() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        let file_path = std::env::temp_dir()
            .join(format!("rts_bootstrap_std_io_{unique}.txt"))
            .display()
            .to_string()
            .replace('\\', "/");

        let source = format!(
            r#"
const written = fs.write("{file_path}", "hello-std");
io.print("write_ok=" + written.ok);
const content = fs.read_to_string("{file_path}");
io.print("read_ok=" + io.is_ok(content));
io.print("value=" + content.value);
io.print("unwrap=" + io.unwrap_or(content, "fallback"));
"#
        );

        let program = compile_source(&source, CompileOptions::default());
        let lines = program
            .ops
            .iter()
            .map(|op| match op {
                BootstrapOp::WriteLine(line) => line.clone(),
            })
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line == "write_ok=true"));
        assert!(lines.iter().any(|line| line == "read_ok=true"));
        assert!(lines.iter().any(|line| line == "value=hello-std"));
        assert!(lines.iter().any(|line| line == "unwrap=hello-std"));

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn supports_global_buffer_and_async_promises() {
        let source = r#"
global.set("name", "rts");
const id = buffer.alloc(8);
buffer.write_text(id, "ok", 0);
io.print("buffer=" + buffer.read_text(id, 0, 2));
const p = task.hash_sha256(global.get("name"));
io.print("promise=" + promise.status(p));
io.print("hash=" + promise.await(p));
"#;

        let program = compile_source(source, CompileOptions::default());
        let lines = program
            .ops
            .iter()
            .map(|op| match op {
                BootstrapOp::WriteLine(line) => line.clone(),
            })
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line == "buffer=ok"));
        assert!(lines.iter().any(|line| line.starts_with("promise=")));
        assert!(lines.iter().any(|line| line.starts_with("hash=")));
    }
}
