use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::module_system::ModuleGraph;

const BOOTSTRAP_MAGIC: &[u8] = b"RTS_BOOTSTRAP_BIN_V1\0";
const OP_WRITE_LINE: u8 = 1;

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub lines_emitted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BootstrapProgram {
    pub ops: Vec<BootstrapOp>,
}

impl BootstrapProgram {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(BOOTSTRAP_MAGIC);
        out.extend_from_slice(&(self.ops.len() as u32).to_le_bytes());

        for op in &self.ops {
            match op {
                BootstrapOp::WriteLine(line) => {
                    out.push(OP_WRITE_LINE);
                    let bytes = line.as_bytes();
                    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                    out.extend_from_slice(bytes);
                }
            }
        }

        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < BOOTSTRAP_MAGIC.len() + std::mem::size_of::<u32>() {
            bail!("bootstrap payload is too short");
        }

        if &bytes[..BOOTSTRAP_MAGIC.len()] != BOOTSTRAP_MAGIC {
            bail!("invalid bootstrap payload magic");
        }

        let mut cursor = BOOTSTRAP_MAGIC.len();
        let op_count = read_u32(bytes, &mut cursor)? as usize;
        let mut ops = Vec::with_capacity(op_count);

        for _ in 0..op_count {
            let opcode = *bytes
                .get(cursor)
                .ok_or_else(|| anyhow::anyhow!("unexpected end of bootstrap payload"))?;
            cursor += 1;

            match opcode {
                OP_WRITE_LINE => {
                    let len = read_u32(bytes, &mut cursor)? as usize;
                    let end = cursor
                        .checked_add(len)
                        .ok_or_else(|| anyhow::anyhow!("bootstrap string length overflow"))?;
                    if end > bytes.len() {
                        bail!("invalid bootstrap string length");
                    }

                    let line = String::from_utf8(bytes[cursor..end].to_vec())
                        .context("bootstrap line is not valid UTF-8")?;
                    cursor = end;
                    ops.push(BootstrapOp::WriteLine(line));
                }
                other => bail!("unknown bootstrap opcode {}", other),
            }
        }

        Ok(Self { ops })
    }
}

#[derive(Debug, Clone)]
pub enum BootstrapOp {
    WriteLine(String),
}

pub fn compile_graph(graph: &ModuleGraph) -> Result<BootstrapProgram> {
    let Some(entry) = graph.entry() else {
        return Ok(BootstrapProgram::default());
    };

    let mut functions = BTreeMap::<String, FunctionDef>::new();
    for module in graph.modules() {
        collect_functions(&module.source, &mut functions);
    }

    let top_level = collect_top_level_statements(&entry.source);
    let mut ops = Vec::new();
    compile_statements(&top_level, &functions, &mut ops, 0);

    Ok(BootstrapProgram { ops })
}

pub fn compile_source(entry_source: &str) -> BootstrapProgram {
    let mut functions = BTreeMap::<String, FunctionDef>::new();
    collect_functions(entry_source, &mut functions);

    let top_level = collect_top_level_statements(entry_source);
    let mut ops = Vec::new();
    compile_statements(&top_level, &functions, &mut ops, 0);

    BootstrapProgram { ops }
}

pub fn execute(program: &BootstrapProgram) -> RunReport {
    let mut report = RunReport::default();

    for op in &program.ops {
        match op {
            BootstrapOp::WriteLine(line) => {
                println!("{line}");
                report.lines_emitted += 1;
            }
        }
    }

    report
}

#[derive(Debug, Clone)]
struct FunctionDef {
    body: Vec<String>,
}

fn compile_statements(
    statements: &[String],
    functions: &BTreeMap<String, FunctionDef>,
    ops: &mut Vec<BootstrapOp>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }

    for raw in statements {
        let line = normalize(raw);
        if line.is_empty() {
            continue;
        }

        if try_emit_call("print", line, &mut |args| {
            ops.push(BootstrapOp::WriteLine(args.join("")));
        }) {
            continue;
        }

        if try_emit_call("console.log", line, &mut |args| {
            if args.is_empty() {
                ops.push(BootstrapOp::WriteLine("Log:".to_string()));
            } else {
                ops.push(BootstrapOp::WriteLine(format!("Log: {}", args.join(" "))));
            }
        }) {
            continue;
        }

        if line.contains(".stdout.write(") {
            if let Some(args) = parse_call_args(line, ".stdout.write") {
                ops.push(BootstrapOp::WriteLine(args.join("")));
                continue;
            }
        }

        if let Some(function_name) = parse_void_call(line) {
            if let Some(function) = functions.get(function_name) {
                compile_statements(&function.body, functions, ops, depth + 1);
            }
        }
    }
}

fn collect_top_level_statements(source: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut depth = 0i32;

    for raw_line in source.lines() {
        let line = strip_comment(raw_line);
        let trimmed = line.trim();

        if trimmed.is_empty() {
            depth += brace_delta(trimmed);
            continue;
        }

        if depth == 0 && !trimmed.starts_with("import ") && !trimmed.starts_with("export ") {
            if !trimmed.starts_with("function ") && !trimmed.starts_with("class ") {
                statements.push(trimmed.to_string());
            }
        }

        depth += brace_delta(trimmed);
    }

    statements
}

fn collect_functions(source: &str, functions: &mut BTreeMap<String, FunctionDef>) {
    let mut in_function = false;
    let mut function_depth = 0i32;
    let mut function_name = String::new();
    let mut body = Vec::<String>::new();

    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim().to_string();

        if !in_function {
            if let Some(name) = parse_function_decl_name(&line) {
                in_function = true;
                function_name = name;
                function_depth = brace_delta(&line);
                body.clear();
                continue;
            }
            continue;
        }

        body.push(line.clone());
        function_depth += brace_delta(&line);

        if function_depth <= 0 {
            let cleaned = strip_trailing_braces(&body);
            functions
                .entry(function_name.clone())
                .or_insert(FunctionDef { body: cleaned });

            in_function = false;
            function_depth = 0;
            function_name.clear();
            body.clear();
        }
    }
}

fn parse_function_decl_name(line: &str) -> Option<String> {
    let trimmed = line.strip_prefix("export ").unwrap_or(line).trim();
    let decl = trimmed.strip_prefix("function ")?;
    let open = decl.find('(')?;
    let name = decl[..open].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_void_call(line: &str) -> Option<&str> {
    let candidate = line.trim_end_matches(';').trim();
    let open = candidate.find('(')?;
    let close = candidate.rfind(')')?;
    if close <= open {
        return None;
    }

    let fn_name = candidate[..open].trim();
    let args = candidate[open + 1..close].trim();

    if fn_name.is_empty() || !args.is_empty() {
        return None;
    }

    if fn_name.contains('.') {
        return None;
    }

    Some(fn_name)
}

fn parse_call_args(line: &str, prefix: &str) -> Option<Vec<String>> {
    let open = line.find(prefix)? + prefix.len();
    let mut tail = &line[open..];
    tail = tail.trim();
    if !tail.starts_with('(') {
        return None;
    }

    let close = tail.rfind(')')?;
    let raw_args = &tail[1..close];

    if raw_args.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut args = Vec::new();
    for chunk in split_top_level(raw_args, ',') {
        args.push(eval_literal(chunk.trim()));
    }

    Some(args)
}

fn try_emit_call<F>(prefix: &str, line: &str, on_emit: &mut F) -> bool
where
    F: FnMut(Vec<String>),
{
    let Some(args) = parse_call_args(line, prefix) else {
        return false;
    };

    on_emit(args);
    true
}

fn split_top_level(input: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut quote = '\0';
    let mut escape = false;

    for (idx, ch) in input.char_indices() {
        if escape {
            escape = false;
            continue;
        }

        if ch == '\\' {
            escape = true;
            continue;
        }

        if quote != '\0' {
            if ch == quote {
                quote = '\0';
            }
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = ch;
            continue;
        }

        if ch == separator {
            parts.push(input[start..idx].trim());
            start = idx + ch.len_utf8();
        }
    }

    parts.push(input[start..].trim());
    parts
}

fn eval_literal(expr: &str) -> String {
    let trimmed = expr.trim();

    if let Some(string) = strip_quotes(trimmed) {
        return string.to_string();
    }

    trimmed.to_string()
}

fn strip_quotes(value: &str) -> Option<&str> {
    if value.len() < 2 {
        return None;
    }

    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        Some(&value[1..value.len() - 1])
    } else {
        None
    }
}

fn strip_trailing_braces(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed == "}" {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn strip_comment(line: &str) -> &str {
    if let Some(index) = line.find("//") {
        &line[..index]
    } else {
        line
    }
}

fn brace_delta(line: &str) -> i32 {
    let open = line.chars().filter(|ch| *ch == '{').count() as i32;
    let close = line.chars().filter(|ch| *ch == '}').count() as i32;
    open - close
}

fn normalize(line: &str) -> &str {
    line.trim()
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
