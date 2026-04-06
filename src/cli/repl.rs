use std::io::{self, Write};

use anyhow::Result;

pub fn command() -> Result<()> {
    println!("RTS REPL (bootstrap). Type :quit to exit.");

    let stdin = io::stdin();

    loop {
        print!("rts> ");
        io::stdout().flush()?;

        let mut line = String::new();
        let bytes_read = stdin.read_line(&mut line)?;

        if bytes_read == 0 {
            break;
        }

        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed == ":quit" || trimmed == ":q" {
            break;
        }

        match crate::parser::parse_source(trimmed) {
            Ok(program) => {
                let mut registry = crate::type_system::TypeRegistry::default();
                let imports = crate::type_system::checker::ImportExports::default();
                if let Err(error) =
                    crate::type_system::checker::check_program(&program, &mut registry, &imports)
                {
                    println!("type error: {error}");
                } else {
                    println!(
                        "ok (items={}, known_types={})",
                        program.items.len(),
                        registry.len()
                    );
                }
            }
            Err(error) => println!("parse error: {error}"),
        }
    }

    Ok(())
}
