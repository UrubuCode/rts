use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

pub fn command(project_name_arg: Option<String>) -> Result<()> {
    let project_name = match project_name_arg {
        Some(value) => normalize_name(value)?,
        None => prompt_project_name()?,
    };

    let project_dir = std::env::current_dir()
        .context("failed to read current directory")?
        .join(&project_name);

    if project_dir.exists() {
        bail!("target directory already exists: {}", project_dir.display());
    }

    std::fs::create_dir_all(project_dir.join("src")).with_context(|| {
        format!(
            "failed to create project directory {}",
            project_dir.join("src").display()
        )
    })?;

    let package_name = to_package_name(&project_name);
    let version = env!("CARGO_PKG_VERSION");

    write_new_file(
        &project_dir.join("src/main.ts"),
        &render_main_ts(&project_name),
    )?;
    write_new_file(
        &project_dir.join("package.json"),
        &render_package_json(&package_name),
    )?;
    write_new_file(
        &project_dir.join("README.md"),
        &render_readme(&project_name, version),
    )?;
    write_new_file(&project_dir.join("tsconfig.json"), &render_tsconfig())?;
    write_new_file(&project_dir.join(".gitignore"), &render_gitignore())?;

    // Gera rts-types/rts.d.ts na raiz do projeto para IntelliSense.
    // A pasta e ignorada pelo .gitignore gerado e pode ser regenerada
    // com `rts emit-types`.
    let types_dir = project_dir.join("rts-types");
    std::fs::create_dir_all(&types_dir)
        .with_context(|| format!("failed to create {}", types_dir.display()))?;
    emit_rts_dts(&types_dir.join("rts.d.ts"))?;

    println!(
        "Project '{}' generated at {}",
        project_name,
        project_dir.display()
    );
    println!("Next steps:");
    println!("  cd {}", project_name);
    println!("  rts src/main.ts");

    Ok(())
}

/// Emits a minimal `rts.d.ts` from the registered ABI SPECS.
///
/// Replaces the legacy split/flat generator while codegen is being rebuilt.
/// The file is a single `declare module "rts"` with one nested namespace per
/// registered SPEC plus the primitive type aliases expected by user code.
fn emit_rts_dts(path: &Path) -> Result<()> {
    use crate::abi::SPECS;
    use crate::abi::member::MemberKind;

    let mut out = String::from("declare module \"rts\" {\n");
    out.push_str(
        "  export type i8 = number;\n  export type u8 = number;\n  export type i16 = number;\n  export type u16 = number;\n  export type i32 = number;\n  export type u32 = number;\n  export type i64 = number;\n  export type u64 = number;\n  export type isize = number;\n  export type usize = number;\n  export type f32 = number;\n  export type f64 = number;\n  export type bool = boolean;\n  export type str = string;\n\n",
    );

    for spec in SPECS {
        out.push_str(&format!("  /** {} */\n", spec.doc));
        out.push_str(&format!("  export namespace {} {{\n", spec.name));
        for member in spec.members {
            out.push_str(&format!("    /** {} */\n", member.doc));
            match member.kind {
                MemberKind::Function | MemberKind::Constructor => {
                    out.push_str(&format!("    export function {};\n", member.ts_signature));
                }
                MemberKind::Constant => {
                    out.push_str(&format!("    export const {};\n", member.ts_signature));
                }
                MemberKind::InstanceMethod => {}
            }
        }
        out.push_str("  }\n\n");
    }

    out.push_str("}\n");
    std::fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn prompt_project_name() -> Result<String> {
    print!("Project name: ");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read project name")?;

    normalize_name(input)
}

fn normalize_name(raw: String) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() {
        bail!("project name cannot be empty");
    }

    if value.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        bail!("project name contains unsupported path characters");
    }

    Ok(value.to_string())
}

fn to_package_name(project_name: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;

    for ch in project_name.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if matches!(ch, '-' | '_' | ' ') {
            '-'
        } else {
            continue;
        };

        if normalized == '-' {
            if previous_dash || out.is_empty() {
                continue;
            }
            previous_dash = true;
            out.push('-');
            continue;
        }

        previous_dash = false;
        out.push(normalized);
    }

    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "rts-app".to_string()
    } else {
        out.to_string()
    }
}

fn write_new_file(path: &Path, content: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;

    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn render_main_ts(project_name: &str) -> String {
    // O entrypoint do RTS e o top-level do modulo.
    // Mantemos o scaffold sem `function main()` para evitar conflito
    // com simbolos reservados do entrypoint nativo.
    format!("import {{ io }} from \"rts\";\n\nio.print(\"hello from {project_name}\");\n")
}

fn render_package_json(package_name: &str) -> String {
    format!(
        "{{\n  \"name\": \"{package_name}\",\n  \"version\": \"0.1.0\",\n  \"main\": \"src/main.ts\",\n  \"dependencies\": {{}}\n}}\n"
    )
}

fn render_readme(project_name: &str, version: &str) -> String {
    format!(
        "### Project {project_name} generated with rts {version}\n\n## Run\n\n```bash\nrts src/main.ts\n```\n"
    )
}

/// `tsconfig.json` gerado para o projeto. Aponta para `rts-types/`
/// na raiz do projeto, gerado por `rts init` e regeneravel com `rts emit-types`.
fn render_tsconfig() -> String {
    // Formato simples — um unico `typeRoots` + `paths` para permitir
    // `import { io } from "rts"` e `import * as fs from "rts:fs"`.
    r#"{
  "compilerOptions": {
    "target": "es2022",
    "module": "esnext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "baseUrl": ".",
    "typeRoots": ["./rts-types"],
    "paths": {
      "rts": ["./rts-types/rts.d.ts"],
      "rts:*": ["./rts-types/*.d.ts"]
    },
    "types": []
  },
  "include": ["src/**/*.ts"]
}
"#
    .to_string()
}

/// `.gitignore` padrao — ignora caches do RTS e binarios compilados
/// na raiz do projeto (convencao do `rts compile`).
fn render_gitignore() -> String {
    r#"# RTS caches
node_modules/
target/
rts-types/

# Compiled binaries (rts compile default output path is project root)
*.exe
*.dll
*.so
*.dylib

# OS
.DS_Store
Thumbs.db
"#
    .to_string()
}
