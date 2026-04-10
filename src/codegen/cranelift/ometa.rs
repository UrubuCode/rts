/// Writer para o formato `.ometa` — Object Metadata.
///
/// `.ometa` é um JSON gerado pelo codegen que mapeia offsets de PC para
/// localizações no arquivo fonte TypeScript. Lido pelo runtime C em modo
/// desenvolvimento para exibir erros com localização precisa.
///
/// Formato:
/// ```json
/// {
///   "version": 1,
///   "mode": "development",
///   "sourceRoot": "/project/src",
///   "sources": ["index.ts"],
///   "locations": {
///     "0x12a3f": { "source": "index.ts", "line": 42, "column": 10 }
///   },
///   "functions": {
///     "_rts_foo": { "offset": 74751, "size": 256, "source": "index.ts", "line": 40 }
///   }
/// }
/// ```
use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct OmetaLocation {
    pub source: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone)]
pub struct OmetaFunction {
    pub offset: u64,
    pub size: u64,
    pub source: String,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct OmetaWriter {
    pub mode: String,
    pub source_root: String,
    sources: Vec<String>,
    locations: HashMap<u64, OmetaLocation>,
    functions: HashMap<String, OmetaFunction>,
}

impl OmetaWriter {
    pub fn new(mode: &str, source_root: &str) -> Self {
        Self {
            mode: mode.to_string(),
            source_root: source_root.to_string(),
            sources: Vec::new(),
            locations: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn add_source(&mut self, source: &str) {
        if !self.sources.contains(&source.to_string()) {
            self.sources.push(source.to_string());
        }
    }

    pub fn add_location(&mut self, pc_offset: u64, source: &str, line: u32, column: u32) {
        self.add_source(source);
        self.locations.insert(
            pc_offset,
            OmetaLocation {
                source: source.to_string(),
                line,
                column,
            },
        );
    }

    pub fn add_function(&mut self, name: &str, offset: u64, size: u64, source: &str, line: u32) {
        self.add_source(source);
        self.functions.insert(
            name.to_string(),
            OmetaFunction {
                offset,
                size,
                source: source.to_string(),
                line,
            },
        );
    }

    pub fn is_empty(&self) -> bool {
        self.locations.is_empty() && self.functions.is_empty()
    }

    /// Serializa para JSON e grava no caminho fornecido (mesmo nome do `.o`, extensão `.ometa`).
    pub fn write_to(&self, obj_path: &Path) -> Result<()> {
        let ometa_path = obj_path.with_extension("ometa");
        let json = self.to_json();
        std::fs::write(&ometa_path, json)?;
        Ok(())
    }

    fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str("  \"version\": 1,\n");
        out.push_str(&format!("  \"mode\": \"{}\",\n", escape_json(&self.mode)));
        out.push_str(&format!(
            "  \"sourceRoot\": \"{}\",\n",
            escape_json(&self.source_root)
        ));

        // sources array
        out.push_str("  \"sources\": [");
        for (i, src) in self.sources.iter().enumerate() {
            if i > 0 { out.push_str(", "); }
            out.push_str(&format!("\"{}\"", escape_json(src)));
        }
        out.push_str("],\n");

        // locations object
        out.push_str("  \"locations\": {\n");
        let mut locs: Vec<_> = self.locations.iter().collect();
        locs.sort_by_key(|(k, _)| *k);
        for (i, (offset, loc)) in locs.iter().enumerate() {
            let comma = if i + 1 < locs.len() { "," } else { "" };
            out.push_str(&format!(
                "    \"0x{:x}\": {{ \"source\": \"{}\", \"line\": {}, \"column\": {} }}{}\n",
                offset, escape_json(&loc.source), loc.line, loc.column, comma
            ));
        }
        out.push_str("  },\n");

        // functions object
        out.push_str("  \"functions\": {\n");
        let mut fns: Vec<_> = self.functions.iter().collect();
        fns.sort_by_key(|(k, _)| k.clone());
        for (i, (name, func)) in fns.iter().enumerate() {
            let comma = if i + 1 < fns.len() { "," } else { "" };
            out.push_str(&format!(
                "    \"{}\": {{ \"offset\": {}, \"size\": {}, \"source\": \"{}\", \"line\": {} }}{}\n",
                escape_json(name), func.offset, func.size, escape_json(&func.source), func.line, comma
            ));
        }
        out.push_str("  }\n");

        out.push('}');
        out
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ometa_writer_serializes_correctly() {
        let mut writer = OmetaWriter::new("development", "/project/src");
        writer.add_location(0x12a3f, "index.ts", 42, 10);
        writer.add_function("_rts_foo", 0x12a00, 256, "index.ts", 40);

        let json = writer.to_json();
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"mode\": \"development\""));
        assert!(json.contains("\"0x12a3f\""));
        assert!(json.contains("\"line\": 42"));
        assert!(json.contains("\"_rts_foo\""));
    }
}
