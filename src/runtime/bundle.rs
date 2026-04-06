use std::path::Path;

use anyhow::{Context, Result};

const RTS_BUNDLE_MARKER: &[u8] = b"RTS_BUNDLE_V1\0";

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

    let payload_len = payload.len() as u64;

    let mut packaged = Vec::with_capacity(
        host_bytes.len() + RTS_BUNDLE_MARKER.len() + std::mem::size_of::<u64>() + payload.len(),
    );

    packaged.extend_from_slice(&host_bytes);
    packaged.extend_from_slice(RTS_BUNDLE_MARKER);
    packaged.extend_from_slice(&payload_len.to_le_bytes());
    packaged.extend_from_slice(payload);

    std::fs::write(output_binary, &packaged)
        .with_context(|| format!("failed to write packaged binary {}", output_binary.display()))?;

    Ok(())
}

pub fn read_embedded_entry_from_current_exe() -> Result<Option<EntryBundle>> {
    let current = std::env::current_exe().context("failed to locate current executable")?;
    let bytes = std::fs::read(&current)
        .with_context(|| format!("failed to read executable {}", current.display()))?;

    let mut search_end = bytes.len();

    while let Some(marker_index) = rfind_marker(&bytes[..search_end], RTS_BUNDLE_MARKER) {
        let Some(length_start) = marker_index.checked_add(RTS_BUNDLE_MARKER.len()) else {
            search_end = marker_index;
            continue;
        };
        let Some(length_end) = length_start.checked_add(std::mem::size_of::<u64>()) else {
            search_end = marker_index;
            continue;
        };

        if length_end > bytes.len() {
            search_end = marker_index;
            continue;
        }

        let mut length_bytes = [0u8; 8];
        length_bytes.copy_from_slice(&bytes[length_start..length_end]);
        let payload_len = u64::from_le_bytes(length_bytes) as usize;

        let Some(payload_end) = length_end.checked_add(payload_len) else {
            search_end = marker_index;
            continue;
        };

        // Only treat the marker as a valid bundle when the payload reaches EOF.
        if payload_end != bytes.len() {
            search_end = marker_index;
            continue;
        }

        let payload = bytes[length_end..payload_end].to_vec();
        return Ok(Some(EntryBundle { payload }));
    }

    Ok(None)
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
