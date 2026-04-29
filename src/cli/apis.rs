//! `rts apis` — prints every namespace and member registered on the ABI.

use anyhow::Result;

use crate::abi::SPECS;
use crate::abi::member::MemberKind;

pub fn command() -> Result<()> {
    println!("RTS Runtime APIs (builtin module \"rts\"):");
    for export in crate::runtime::rts_exports() {
        println!("  - {export}");
    }

    println!();
    println!("RTS Namespace Catalog (Rust -> Cranelift):");
    for spec in SPECS {
        println!("  - {}: {}", spec.name, spec.doc);
        for member in spec.members {
            let kind = match member.kind {
                MemberKind::Function | MemberKind::Constructor => "fn",
                MemberKind::Constant => "const",
                MemberKind::InstanceMethod => "method",
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
