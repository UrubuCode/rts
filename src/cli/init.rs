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

    // Gera os .d.ts em `node_modules/.rts/builtin/rts-types/` para que
    // o VSCode tenha IntelliSense de primeiro dia. O tsconfig aponta
    // exatamente para esse diretorio via `typeRoots`.
    let types_dir = project_dir
        .join("node_modules")
        .join(".rts")
        .join("builtin")
        .join("rts-types");
    std::fs::create_dir_all(&types_dir)
        .with_context(|| format!("failed to create {}", types_dir.display()))?;
    crate::namespaces::emit_split_typescript_declarations(&types_dir)?;
    crate::namespaces::emit_typescript_declarations(&types_dir.join("rts.d.ts"))?;

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
    // O RTS ja invoca `main()` automaticamente como entry point.
    // Nao incluir `main();` no top-level — isso cria uma chamada recursiva
    // com o bootstrap do runtime.
    format!(
        "import {{ io }} from \"rts\";\n\nfunction main(): void {{\n  io.print(\"hello from {project_name}\");\n}}\n"
    )
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

/// `tsconfig.json` gerado para o projeto. Aponta o resolver do TypeScript
/// para os types em `node_modules/.rts/builtin/rts-types/`, que sao
/// gerados pelo `rts init` e podem ser regenerados com `rts emit-types`.
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
    "typeRoots": ["./node_modules/.rts/builtin/rts-types"],
    "paths": {
      "rts": ["./node_modules/.rts/builtin/rts-types/rts.d.ts"],
      "rts:*": ["./node_modules/.rts/builtin/rts-types/*.d.ts"]
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
