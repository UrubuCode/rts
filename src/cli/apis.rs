use anyhow::Result;

pub fn command() -> Result<()> {
    println!("RTS Runtime APIs (builtin module \"rts\"):");
    for export in crate::runtime::rts_exports() {
        println!("  - {export}");
    }

    println!();
    println!("RTS Namespace Catalog (Rust -> Cranelift):");
    for namespace in crate::namespaces::documentation_catalog() {
        println!("  - {}: {}", namespace.namespace, namespace.doc);
        for function in namespace.functions {
            println!(
                "      * {} ({}) -> {}",
                function.callee, function.ts_signature, function.doc
            );
        }
    }

    println!();
    println!("RTS Compiler Dependencies (Cargo):");
    for dependency in crate::runtime::compiler_dependencies() {
        println!("  - {dependency}");
    }

    println!();
    println!("RTS Pending Runtime APIs:");
    for item in crate::runtime::rts_pending_apis() {
        println!("  - {item}");
    }

    Ok(())
}
