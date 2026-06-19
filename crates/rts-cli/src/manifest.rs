//! Minimal `package.json` parsing for `rts install`. The full module-graph
//! manifest resolver lived in the old engine (deleted at the P5 cutover); only the
//! raw deserialization + JSON-comment stripping the installer needs is kept here.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The subset of `package.json` the installer reads.
#[derive(Debug, Deserialize)]
pub struct RawPackageManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub main: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// Strip `//` line comments outside of strings (JSONC → JSON).
pub fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && matches!(chars.peek(), Some('/')) {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    output
}
