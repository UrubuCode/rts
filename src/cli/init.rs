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
    format!(
        "import {{ io }} from \"rts\";\n\nfunction main(): void {{\n  io.print(\"hello from {project_name}\");\n}}\n\nmain();\n"
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
