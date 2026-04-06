use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
    let colors = Colors::new();

    println!(
        "{} {}",
        colors.paint("1;34", "RTS"),
        colors.paint("2", "build pipeline started")
    );

    let summary = crate::compile_file_with_options(&input, &output, options)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    println!(
        "{} {}",
        colors.paint("1;32", "Build complete:"),
        colors.paint("1", &summary.binary_file.display().to_string())
    );
    println!(
        "  {} profile={} modules={} types={} functions={}",
        colors.paint("36", "stats"),
        summary.profile,
        summary.compiled_modules,
        summary.discovered_types,
        summary.lowered_functions
    );
    println!(
        "  {} backend={} format={}",
        colors.paint("35", "link"),
        summary.link_backend,
        summary.link_format
    );
    println!(
        "  {} app.o={} runtime.o(embedded)={} final={}",
        colors.paint("33", "size"),
        format_bytes(summary.app_object_bytes as u64),
        format_bytes(summary.runtime_object_bytes as u64),
        format_bytes(summary.binary_bytes)
    );

    emit_object_diagnostics(&summary.object_file, &colors);

    Ok(())
}

fn emit_object_diagnostics(path: &Path, colors: &Colors) {
    match read_object_section_sizes(path) {
        Ok(mut sections) if !sections.is_empty() => {
            sections.sort_by(|a, b| b.1.cmp(&a.1));
            println!(
                "  {} {}",
                colors.paint("94", "object sections"),
                colors.paint("2", &path.display().to_string())
            );

            for (name, size) in sections.into_iter().take(8) {
                println!(
                    "    {:<20} {}",
                    colors.paint("90", &name),
                    colors.paint("96", &format_bytes(size))
                );
            }
        }
        Ok(_) => {
            println!(
                "  {} {}",
                colors.paint("94", "object sections"),
                colors.paint("2", "none")
            );
        }
        Err(error) => {
            println!(
                "  {} {}",
                colors.paint("31", "object diagnostics unavailable:"),
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

struct Colors {
    enabled: bool,
}

impl Colors {
    fn new() -> Self {
        let enabled = std::env::var_os("NO_COLOR").is_none();
        Self { enabled }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}
