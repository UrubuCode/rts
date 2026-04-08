use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;
use console::style;
use object::{Object, ObjectSection};

use crate::compile_options::CompileOptions;

pub fn command(
    input_arg: Option<String>,
    output_arg: Option<String>,
    mut options: CompileOptions,
) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/hello_world.ts"));

    let output = output_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/rts_app"));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    options.emit_module_progress = true;

    // Cabeçalho moderno com ícone e gradiente de cores
    println!(
        "{} {}",
        "⚡".bright_blue().bold(),
        "RTS Build Pipeline".bright_blue().bold()
    );
    println!("{}", "─".repeat(50).dimmed());

    let summary = crate::compile_file_with_options(&input, &output, options)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    // Sucesso com ícone
    println!(
        "\n{} {}\n{}",
        "✔".green().bold(),
        "Build completed successfully".green().bold(),
        format!("  {}", summary.binary_file.display()).dimmed()
    );

    // Layout de tabela para estatísticas
    println!("\n{}", "📊 Build Summary".cyan().bold());
    println!("{}", "─".repeat(50).dimmed());

    // Função auxiliar para imprimir linha alinhada
    let print_row = |label: &str, value: String| {
        println!("  {:<20} {}", label.dimmed(), value);
    };

    print_row("Profile", style(&summary.profile).yellow().to_string());
    print_row(
        "Modules",
        style(summary.compiled_modules).cyan().to_string(),
    );
    print_row("Types", style(summary.discovered_types).cyan().to_string());
    print_row(
        "Functions",
        style(summary.lowered_functions).cyan().to_string(),
    );

    println!("{}", "  ──────────────────────────────────".dimmed());

    let runtime_ns = if summary.runtime_namespaces.is_empty() {
        "none".to_string()
    } else {
        summary.runtime_namespaces.join(", ")
    };
    print_row(
        "Runtime namespaces",
        style(runtime_ns).magenta().to_string(),
    );
    print_row(
        "Runtime functions",
        style(summary.runtime_functions).magenta().to_string(),
    );
    print_row(
        "Cache directory",
        style(summary.runtime_cache_dir.display()).dim().to_string(),
    );

    println!("{}", "  ──────────────────────────────────".dimmed());

    print_row(
        "Link backend",
        style(&summary.link_backend).blue().to_string(),
    );
    print_row(
        "Link format",
        style(&summary.link_format).blue().to_string(),
    );

    println!("{}", "  ──────────────────────────────────".dimmed());

    print_row(
        "Dependency objects",
        style(summary.dependency_objects).green().to_string(),
    );
    print_row(
        "Cache hits/misses",
        format!(
            "{}/{}",
            style(summary.cache_hits).green(),
            style(summary.cache_misses).red()
        ),
    );

    println!("{}", "  ──────────────────────────────────".dimmed());

    // Tamanhos dos arquivos com ícones
    println!("  {} {}", "📦".yellow(), "Size breakdown".yellow().bold());
    println!(
        "    {:<18} {}",
        "App object".dimmed(),
        format_bytes(summary.app_object_bytes as u64).cyan()
    );
    let runtime_size = if summary.runtime_object_bytes == 0 {
        "disabled".dimmed().to_string()
    } else {
        format_bytes(summary.runtime_object_bytes as u64)
            .magenta()
            .to_string()
    };
    println!("    {:<18} {}", "Runtime".dimmed(), runtime_size);
    println!(
        "    {:<18} {}",
        "Final binary".dimmed(),
        format_bytes(summary.binary_bytes).green().bold()
    );

    emit_object_diagnostics(&summary.object_file);

    Ok(())
}

fn emit_object_diagnostics(path: &Path) {
    match read_object_section_sizes(path) {
        Ok(mut sections) if !sections.is_empty() => {
            sections.sort_by(|a, b| b.1.cmp(&a.1));
            println!(
                "\n{} {}",
                "🔬".bright_blue(),
                "Object sections".bright_blue().bold()
            );
            println!("   {}", path.display().to_string().dimmed());

            for (name, size) in sections.into_iter().take(8) {
                let bar = generate_usage_bar(size, 1024 * 1024); // escala relativa a 1MB
                println!(
                    "    {:<20} {} {}",
                    name.dimmed(),
                    format_bytes(size).cyan(),
                    bar
                );
            }
        }
        Ok(_) => {
            println!("\n{} {}", "🔬".bright_blue(), "No object sections".dimmed());
        }
        Err(error) => {
            println!(
                "\n{} {} {}",
                "⚠".yellow(),
                "Object diagnostics unavailable:".red(),
                error
            );
        }
    }
}

fn read_object_section_sizes(path: &Path) -> Result<Vec<(String, u64)>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read object file {}", path.display()))?;
    let file = object::File::parse(&*bytes).map_err(|error| {
        anyhow::anyhow!("failed to parse object file {}: {error}", path.display())
    })?;

    let mut sections = Vec::new();
    for section in file.sections() {
        let name = section.name().unwrap_or("<invalid>");
        let size = section.size();
        sections.push((name.to_string(), size));
    }

    Ok(sections)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Gera uma barra de uso proporcional (estilo moderna)
fn generate_usage_bar(value: u64, max: u64) -> String {
    let ratio = (value as f64 / max as f64).min(1.0);
    let bar_width = 20;
    let filled = (ratio * bar_width as f64).round() as usize;
    let empty = bar_width - filled;
    format!(
        "[{}{}]",
        "█".repeat(filled).green(),
        "░".repeat(empty).dimmed()
    )
}
