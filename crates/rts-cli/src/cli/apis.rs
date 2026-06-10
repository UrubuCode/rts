//! `rts apis` — prints every namespace and member registered on the ABI.

use anyhow::Result;

use crate::abi::member::MemberKind;
use crate::abi::registry_specs_ordered;

pub fn command() -> Result<()> {
    println!("RTS Runtime APIs (builtin module \"rts\"):");
    for export in crate::runtime::rts_exports() {
        println!("  - {export}");
    }

    println!();
    println!("RTS Namespace Catalog (Rust -> Cranelift):");
    // Itera o registry (const seed + módulos do builder/Fase 2), não o const
    // `SPECS` — que hoje só guarda gc + collections.
    for spec in registry_specs_ordered() {
        println!("  - {}: {}", spec.name, spec.doc);
        for member in spec.members {
            let kind = match member.kind {
                MemberKind::Function | MemberKind::Constructor => "fn",
                MemberKind::Constant => "const",
                MemberKind::InstanceMethod => "method",
                MemberKind::StaticMethod => "static",
                MemberKind::InstanceGetter => "getter",
                MemberKind::InstanceSetter => "setter",
                MemberKind::VarGetter => "var-get",
                MemberKind::VarSetter => "var-set",
            };
            println!(
                "      * [{kind}] {sig}  -> {symbol}  // {doc}",
                sig = member.ts_signature,
                symbol = member.symbol,
                doc = member.doc,
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
