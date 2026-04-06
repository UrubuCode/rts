use anyhow::{Context, Result, bail};
use object::write::{Object, StandardSegment, Symbol, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, Object as ObjectReader, ObjectSection, SectionKind,
    SymbolFlags, SymbolKind, SymbolScope,
};

use crate::runtime::bootstrap::BootstrapProgram;
use crate::runtime::namespaces::NamespaceUsage;

const PROGRAM_SECTION: &[u8] = b".rts.program";
const USAGE_SECTION: &[u8] = b".rts.usage";
const NAMESPACE_SECTION: &[u8] = b".rts.namespaces";
const PROGRAM_SYMBOL: &[u8] = b"__rts_program";
const USAGE_SYMBOL: &[u8] = b"__rts_usage";
const NAMESPACE_SYMBOL: &[u8] = b"__rts_namespaces";

pub fn build_for_sources<'a>(
    sources: impl IntoIterator<Item = &'a str>,
    program: &BootstrapProgram,
) -> Result<Vec<u8>> {
    let usage = NamespaceUsage::from_sources(sources);
    build_runtime_object(program, &usage)
}

pub fn decode_runtime_object(payload: &[u8]) -> Result<BootstrapProgram> {
    let file = object::File::parse(payload)
        .map_err(|error| anyhow::anyhow!("invalid runtime object payload: {error}"))?;

    for section in file.sections() {
        let Ok(name) = section.name() else {
            continue;
        };

        if name == ".rts.program" {
            let bytes = section
                .data()
                .context("failed to read .rts.program section from runtime object")?;
            return BootstrapProgram::decode(bytes);
        }
    }

    bail!("runtime object payload is missing .rts.program section")
}

fn build_runtime_object(program: &BootstrapProgram, usage: &NamespaceUsage) -> Result<Vec<u8>> {
    let target = HostObjectTarget::resolve()?;
    let mut object = Object::new(
        target.binary_format,
        target.architecture,
        Endianness::Little,
    );

    let runtime_segment = object.segment_name(StandardSegment::Data).to_vec();

    let program_section = object.add_section(
        runtime_segment.clone(),
        PROGRAM_SECTION.to_vec(),
        SectionKind::ReadOnlyData,
    );
    let program_bytes = program.encode();
    let program_offset = object.append_section_data(program_section, &program_bytes, 1);
    object.add_symbol(Symbol {
        name: PROGRAM_SYMBOL.to_vec(),
        value: program_offset,
        size: program_bytes.len() as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Compilation,
        weak: false,
        section: SymbolSection::Section(program_section),
        flags: SymbolFlags::None,
    });

    let usage_section = object.add_section(
        runtime_segment.clone(),
        USAGE_SECTION.to_vec(),
        SectionKind::ReadOnlyData,
    );
    let usage_bytes = usage
        .enabled_functions()
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let usage_offset = object.append_section_data(usage_section, &usage_bytes, 1);
    object.add_symbol(Symbol {
        name: USAGE_SYMBOL.to_vec(),
        value: usage_offset,
        size: usage_bytes.len() as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Compilation,
        weak: false,
        section: SymbolSection::Section(usage_section),
        flags: SymbolFlags::None,
    });

    let namespace_section = object.add_section(
        runtime_segment,
        NAMESPACE_SECTION.to_vec(),
        SectionKind::ReadOnlyData,
    );
    let namespace_bytes = usage
        .enabled_namespaces()
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let namespace_offset = object.append_section_data(namespace_section, &namespace_bytes, 1);
    object.add_symbol(Symbol {
        name: NAMESPACE_SYMBOL.to_vec(),
        value: namespace_offset,
        size: namespace_bytes.len() as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Compilation,
        weak: false,
        section: SymbolSection::Section(namespace_section),
        flags: SymbolFlags::None,
    });

    object
        .write()
        .map_err(|error| anyhow::anyhow!("failed to emit runtime object payload: {error}"))
}

#[derive(Debug, Clone, Copy)]
struct HostObjectTarget {
    binary_format: BinaryFormat,
    architecture: Architecture,
}

impl HostObjectTarget {
    fn resolve() -> Result<Self> {
        let binary_format = match std::env::consts::OS {
            "windows" => BinaryFormat::Coff,
            "macos" => BinaryFormat::MachO,
            _ => BinaryFormat::Elf,
        };

        let architecture = match std::env::consts::ARCH {
            "x86_64" => Architecture::X86_64,
            "aarch64" => Architecture::Aarch64,
            "x86" => Architecture::I386,
            other => bail!("unsupported host architecture for runtime object payload: {other}"),
        };

        Ok(Self {
            binary_format,
            architecture,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::bootstrap::BootstrapOp;

    use super::{build_for_sources, decode_runtime_object};

    #[test]
    fn runtime_object_roundtrip_program() {
        let program = crate::runtime::bootstrap::BootstrapProgram {
            ops: vec![BootstrapOp::WriteLine("hello".to_string())],
            traces: vec![None],
        };

        let payload = build_for_sources(["io.print('hello')", "const v = 1"], &program)
            .expect("runtime object payload should build");
        let decoded =
            decode_runtime_object(&payload).expect("runtime object payload should decode");

        assert_eq!(decoded.ops.len(), 1);
        match &decoded.ops[0] {
            BootstrapOp::WriteLine(value) => assert_eq!(value, "hello"),
        }
    }
}
