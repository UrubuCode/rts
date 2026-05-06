use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use object::write::{Object, StandardSegment, Symbol, SymbolSection, pe as pe_writer};
use object::{
    Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope, pe,
};

#[derive(Debug, Clone)]
pub struct LinkedArtifact {
    pub path: PathBuf,
    pub format: String,
}

pub fn link(object_path: &Path, output_path: &Path) -> Result<LinkedArtifact> {
    let payload = std::fs::read(object_path)
        .with_context(|| format!("failed to read object payload {}", object_path.display()))?;

    let target = LinkTarget::host()?;
    let final_path = normalize_output_path(output_path, target.flavor);

    let (bytes, format) = match target.flavor {
        LinkFlavor::Coff => (
            build_windows_pe_executable(&payload, target)?,
            "pe-exe".to_string(),
        ),
        LinkFlavor::Elf | LinkFlavor::MachO => (
            build_binary_container(&payload, target)?,
            format!("{}-object", target.format_name()),
        ),
    };

    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    std::fs::write(&final_path, &bytes)
        .with_context(|| format!("failed to write binary {}", final_path.display()))?;

    Ok(LinkedArtifact {
        path: final_path,
        format,
    })
}

fn build_windows_pe_executable(payload: &[u8], target: LinkTarget) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();

    let mut writer = pe_writer::Writer::new(target.is_64(), 0x1000, 0x200, &mut buffer);

    writer.reserve_dos_header_and_stub();
    writer.reserve_nt_headers(16);
    let section_count = if payload.is_empty() { 1 } else { 2 };
    writer.reserve_section_headers(section_count);

    let text = target.entry_stub();
    let text_range = writer.reserve_text_section(text.len() as u32);

    let payload_range = if payload.is_empty() {
        None
    } else {
        Some(writer.reserve_rdata_section(payload.len() as u32))
    };

    writer
        .write_dos_header_and_stub()
        .map_err(|error| anyhow::anyhow!("failed to write PE DOS header: {error}"))?;

    writer.write_nt_headers(pe_writer::NtHeaders {
        machine: target.windows_machine()?,
        time_date_stamp: unix_timestamp_u32(),
        characteristics: pe::IMAGE_FILE_EXECUTABLE_IMAGE | pe::IMAGE_FILE_LARGE_ADDRESS_AWARE,
        major_linker_version: 1,
        minor_linker_version: 0,
        address_of_entry_point: text_range.virtual_address,
        image_base: target.default_image_base(),
        major_operating_system_version: 6,
        minor_operating_system_version: 0,
        major_image_version: 0,
        minor_image_version: 0,
        major_subsystem_version: 6,
        minor_subsystem_version: 0,
        subsystem: pe::IMAGE_SUBSYSTEM_WINDOWS_CUI,
        dll_characteristics: target.windows_dll_characteristics(),
        size_of_stack_reserve: 1024 * 1024,
        size_of_stack_commit: 0x1000,
        size_of_heap_reserve: 1024 * 1024,
        size_of_heap_commit: 0x1000,
    });

    writer.write_section_headers();
    writer.write_section(text_range.file_offset, &text);

    if let Some(range) = payload_range {
        writer.write_section(range.file_offset, payload);
    }

    Ok(buffer)
}

fn build_binary_container(payload: &[u8], target: LinkTarget) -> Result<Vec<u8>> {
    let mut object = Object::new(
        target.binary_format(),
        target.architecture(),
        target.endianness(),
    );

    let text_section = object.add_section(
        object.segment_name(StandardSegment::Text).to_vec(),
        b".text".to_vec(),
        SectionKind::Text,
    );

    let entry_stub = target.entry_stub();
    let entry_offset = object.append_section_data(text_section, &entry_stub, 16);

    let rodata_section = object.add_section(
        object.segment_name(StandardSegment::Data).to_vec(),
        b".rts.payload".to_vec(),
        SectionKind::ReadOnlyData,
    );

    let payload_offset = object.append_section_data(rodata_section, payload, 1);

    let entry_name = target.entry_symbol_name();
    object.add_symbol(Symbol {
        name: entry_name.as_bytes().to_vec(),
        value: entry_offset,
        size: entry_stub.len() as u64,
        kind: SymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(text_section),
        flags: SymbolFlags::None,
    });

    object.add_symbol(Symbol {
        name: b"__rts_object_payload".to_vec(),
        value: payload_offset,
        size: payload.len() as u64,
        kind: SymbolKind::Data,
        scope: SymbolScope::Compilation,
        weak: false,
        section: SymbolSection::Section(rodata_section),
        flags: SymbolFlags::None,
    });

    object
        .write()
        .map_err(|error| anyhow::anyhow!("failed to build binary container: {error}"))
}

fn normalize_output_path(path: &Path, flavor: LinkFlavor) -> PathBuf {
    if matches!(flavor, LinkFlavor::Coff) && path.extension().is_none() {
        return path.with_extension("exe");
    }

    path.to_path_buf()
}

fn unix_timestamp_u32() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as u32)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy)]
enum LinkFlavor {
    Coff,
    Elf,
    MachO,
}

#[derive(Debug, Clone, Copy)]
struct LinkTarget {
    flavor: LinkFlavor,
    architecture: Architecture,
    endianness: Endianness,
}

impl LinkTarget {
    fn host() -> Result<Self> {
        let flavor = host_flavor();
        let architecture = host_architecture()?;

        Ok(Self {
            flavor,
            architecture,
            endianness: Endianness::Little,
        })
    }

    fn binary_format(self) -> BinaryFormat {
        match self.flavor {
            LinkFlavor::Coff => BinaryFormat::Coff,
            LinkFlavor::Elf => BinaryFormat::Elf,
            LinkFlavor::MachO => BinaryFormat::MachO,
        }
    }

    fn format_name(self) -> &'static str {
        match self.flavor {
            LinkFlavor::Coff => "coff",
            LinkFlavor::Elf => "elf",
            LinkFlavor::MachO => "mach-o",
        }
    }

    fn architecture(self) -> Architecture {
        self.architecture
    }

    fn endianness(self) -> Endianness {
        self.endianness
    }

    fn is_64(self) -> bool {
        matches!(
            self.architecture,
            Architecture::X86_64 | Architecture::Aarch64
        )
    }

    fn windows_machine(self) -> Result<u16> {
        match self.architecture {
            Architecture::X86_64 => Ok(pe::IMAGE_FILE_MACHINE_AMD64),
            Architecture::Aarch64 => Ok(pe::IMAGE_FILE_MACHINE_ARM64),
            Architecture::I386 => Ok(pe::IMAGE_FILE_MACHINE_I386),
            other => bail!("unsupported Windows machine architecture: {other:?}"),
        }
    }

    fn default_image_base(self) -> u64 {
        if self.is_64() {
            0x0000_0001_4000_0000
        } else {
            0x0040_0000
        }
    }

    fn windows_dll_characteristics(self) -> u16 {
        pe::IMAGE_DLLCHARACTERISTICS_NX_COMPAT
    }

    fn entry_symbol_name(self) -> &'static str {
        match self.flavor {
            LinkFlavor::MachO => "_main",
            LinkFlavor::Coff | LinkFlavor::Elf => "_start",
        }
    }

    fn entry_stub(self) -> Vec<u8> {
        match self.architecture {
            Architecture::X86_64 | Architecture::I386 => vec![0x31, 0xC0, 0xC3], // xor eax, eax; ret
            Architecture::Aarch64 => vec![0x00, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6],
            _ => vec![0x00],
        }
    }
}

fn host_flavor() -> LinkFlavor {
    match std::env::consts::OS {
        "windows" => LinkFlavor::Coff,
        "macos" => LinkFlavor::MachO,
        _ => LinkFlavor::Elf,
    }
}

fn host_architecture() -> Result<Architecture> {
    match std::env::consts::ARCH {
        "x86_64" => Ok(Architecture::X86_64),
        "aarch64" => Ok(Architecture::Aarch64),
        "x86" => Ok(Architecture::I386),
        other => bail!("unsupported host architecture for linker backend: {other}"),
    }
}
