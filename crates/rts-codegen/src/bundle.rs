use std::path::Path;

use anyhow::{Context, Result, bail};

const RTS_BUNDLE_MARKER: &[u8] = b"RTS_BUNDLE\0";
const RTS_ENTRY_NAME: &str = "runtime.o";

#[derive(Debug, Clone)]
pub struct EntryBundle {
    pub payload: Vec<u8>,
}

pub fn package_bootstrap_payload(output_binary: &Path, payload: &[u8]) -> Result<()> {
    let host = std::env::current_exe().context("failed to locate current RTS executable")?;
    let host_bytes = std::fs::read(&host)
        .with_context(|| format!("failed to read host executable {}", host.display()))?;

    if let Some(parent) = output_binary.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let packaged = append_named_entry(&host_bytes, RTS_ENTRY_NAME, payload)?;
    std::fs::write(output_binary, &packaged).with_context(|| {
        format!(
            "failed to write packaged binary {}",
            output_binary.display()
        )
    })?;

    Ok(())
}

pub fn read_embedded_entry_from_current_exe() -> Result<Option<EntryBundle>> {
    let current = std::env::current_exe().context("failed to locate current executable")?;
    let bytes = std::fs::read(&current)
        .with_context(|| format!("failed to read executable {}", current.display()))?;

    Ok(parse_embedded_entry_from_bytes(&bytes, RTS_ENTRY_NAME)
        .map(|payload| EntryBundle { payload }))
}

fn append_named_entry(host_bytes: &[u8], entry_name: &str, payload: &[u8]) -> Result<Vec<u8>> {
    if !entry_name.is_ascii() {
        bail!("bundle entry name must be ASCII: '{}'", entry_name);
    }

    let name_bytes = entry_name.as_bytes();
    if name_bytes.len() > u16::MAX as usize {
        bail!("bundle entry name is too long");
    }

    let payload_len = payload.len() as u64;
    let mut packaged = Vec::with_capacity(
        host_bytes.len()
            + RTS_BUNDLE_MARKER.len()
            + std::mem::size_of::<u16>()
            + name_bytes.len()
            + std::mem::size_of::<u64>()
            + payload.len(),
    );

    packaged.extend_from_slice(host_bytes);
    packaged.extend_from_slice(RTS_BUNDLE_MARKER);
    packaged.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    packaged.extend_from_slice(name_bytes);
    packaged.extend_from_slice(&payload_len.to_le_bytes());
    packaged.extend_from_slice(payload);

    Ok(packaged)
}

fn parse_embedded_entry_from_bytes(bytes: &[u8], expected_name: &str) -> Option<Vec<u8>> {
    let expected_name_bytes = expected_name.as_bytes();
    let mut search_end = bytes.len();

    while let Some(marker_index) = rfind_marker(&bytes[..search_end], RTS_BUNDLE_MARKER) {
        let mut cursor = marker_index + RTS_BUNDLE_MARKER.len();

        let Some(name_len_end) = cursor.checked_add(std::mem::size_of::<u16>()) else {
            search_end = marker_index;
            continue;
        };
        if name_len_end > bytes.len() {
            search_end = marker_index;
            continue;
        }

        let mut name_len_bytes = [0u8; 2];
        name_len_bytes.copy_from_slice(&bytes[cursor..name_len_end]);
        let name_len = u16::from_le_bytes(name_len_bytes) as usize;
        cursor = name_len_end;

        let Some(name_end) = cursor.checked_add(name_len) else {
            search_end = marker_index;
            continue;
        };
        if name_end > bytes.len() {
            search_end = marker_index;
            continue;
        }
        let entry_name = &bytes[cursor..name_end];
        cursor = name_end;

        let Some(payload_len_end) = cursor.checked_add(std::mem::size_of::<u64>()) else {
            search_end = marker_index;
            continue;
        };
        if payload_len_end > bytes.len() {
            search_end = marker_index;
            continue;
        }

        let mut payload_len_bytes = [0u8; 8];
        payload_len_bytes.copy_from_slice(&bytes[cursor..payload_len_end]);
        let payload_len = u64::from_le_bytes(payload_len_bytes) as usize;
        cursor = payload_len_end;

        let Some(payload_end) = cursor.checked_add(payload_len) else {
            search_end = marker_index;
            continue;
        };
        if payload_end != bytes.len() {
            search_end = marker_index;
            continue;
        }

        if entry_name == expected_name_bytes {
            return Some(bytes[cursor..payload_end].to_vec());
        }

        search_end = marker_index;
    }

    None
}

fn rfind_marker(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }

    let last = haystack.len() - needle.len();
    (0..=last)
        .rev()
        .find(|&index| &haystack[index..index + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::{RTS_ENTRY_NAME, append_named_entry, parse_embedded_entry_from_bytes};

    #[test]
    fn roundtrip_embedded_runtime_object_payload() {
        let host = b"HOST-EXE";
        let payload = b"OBJ";
        let packed = append_named_entry(host, RTS_ENTRY_NAME, payload).expect("pack");

        let decoded = parse_embedded_entry_from_bytes(&packed, RTS_ENTRY_NAME).expect("decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn ignores_non_matching_entry_name() {
        let host = b"HOST-EXE";
        let payload = b"payload";
        let packed = append_named_entry(host, "other.rtsb", payload).expect("pack");

        let decoded = parse_embedded_entry_from_bytes(&packed, RTS_ENTRY_NAME);
        assert!(decoded.is_none());
    }

    #[test]
    fn rejects_non_ascii_entry_names() {
        let host = b"HOST-EXE";
        let payload = b"payload";
        let result = append_named_entry(host, "m\u{00E1}in.rtsb", payload);
        assert!(result.is_err());
    }
}
