use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::abi::member::MemberKind;
use crate::abi::SPECS;

pub fn command(output: Option<String>) -> Result<()> {
    let path = resolve_output(output)?;
    let content = generate();
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

fn resolve_output(arg: Option<String>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(PathBuf::from(p));
    }
    // Default: builtin/rts-types/rts.d.ts relative to cwd (repo root when
    // run as `cargo run -- emit-types` or `rts emit-types` from the workspace).
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    Ok(cwd.join("builtin").join("rts-types").join("rts.d.ts"))
}

pub fn generate() -> String {
    let mut out = String::with_capacity(8192);

    out.push_str("declare module \"rts\" {\n");
    push_primitive_aliases(&mut out);

    for spec in SPECS {
        push_namespace(&mut out, spec.name, spec.doc, spec.members, "  ");
        out.push('\n');
    }

    out.push_str("}\n");
    out
}

fn push_primitive_aliases(out: &mut String) {
    for alias in &["i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "isize", "usize", "f32", "f64"] {
        out.push_str(&format!("  export type {alias} = number;\n"));
    }
    out.push_str("  export type bool = boolean;\n");
    out.push_str("  export type str = string;\n");
    out.push('\n');
}

fn push_namespace(
    out: &mut String,
    name: &str,
    doc: &str,
    members: &[crate::abi::NamespaceMember],
    indent: &str,
) {
    let inner = format!("{indent}  ");

    // namespace doc
    out.push_str(&format!("{indent}/**\n"));
    out.push_str(&format!("{indent} * {doc}\n"));
    out.push_str(&format!("{indent} */\n"));
    out.push_str(&format!("{indent}export namespace {name} {{\n"));

    for member in members {
        // member doc
        out.push_str(&format!("{inner}/**\n"));
        out.push_str(&format!("{inner} * {}\n", member.doc));
        out.push_str(&format!("{inner} */\n"));

        match member.kind {
            MemberKind::Function => {
                out.push_str(&format!(
                    "{inner}export function {};\n",
                    member.ts_signature
                ));
            }
            MemberKind::Constant => {
                out.push_str(&format!(
                    "{inner}export const {};\n",
                    member.ts_signature
                ));
            }
        }
    }

    out.push_str(&format!("{indent}}}\n"));
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn rts_dts_in_sync_with_specs() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let committed = manifest.join("builtin/rts-types/rts.d.ts");
        super::check(&committed).expect("builtin/rts-types/rts.d.ts out of sync — run `rts emit-types`");
    }
}

/// Checks whether the committed file at `path` matches what `generate()`
/// would produce. Returns `Ok(())` if in sync, `Err` with a diff summary if
/// not. Used by tests / CI.
pub fn check(path: &Path) -> Result<()> {
    let expected = generate();
    let actual = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    if actual == expected {
        return Ok(());
    }

    // Report first diverging line for a quick diagnosis.
    for (i, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
        if a != e {
            anyhow::bail!(
                "{} is out of sync with SPECS (first diff at line {}):\n  committed: {a:?}\n  generated: {e:?}\nRun `rts emit-types` to regenerate.",
                path.display(),
                i + 1
            );
        }
    }

    let al = actual.lines().count();
    let el = expected.lines().count();
    anyhow::bail!(
        "{} is out of sync with SPECS (committed {} lines, generated {} lines).\nRun `rts emit-types` to regenerate.",
        path.display(),
        al,
        el
    );
}
