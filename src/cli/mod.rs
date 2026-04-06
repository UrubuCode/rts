pub mod build;
pub mod repl;
pub mod run;

use anyhow::Result;

pub fn dispatch<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    let _bin_name = args.next().unwrap_or_else(|| "rts".to_string());

    match args.next().as_deref() {
        Some("build") => build::command(args.next(), args.next()),
        Some("run") => run::command(args.next()),
        Some("repl") => repl::command(),
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => {
            eprintln!("Unknown command: {other}");
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("RTS compiler bootstrap CLI");
    println!("Usage:");
    println!("  rts build [input.(rts|ts)] [output]");
    println!("  rts run [input.(rts|ts)]");
    println!("  rts repl");
}
