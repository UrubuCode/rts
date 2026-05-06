//! `rts clean` — limpa o cache de objects gerado por builds anteriores.
//!
//! Remove os dois caminhos de cache que o RTS pode ter gerado:
//! - `node_modules/.rts/` (padrao a partir da Etapa 5 — objs + launcher)
//! - `target/.deps/` e `target/.launcher/` (legado, anterior a Etapa 5)
//!
//! Nao mexe em `target/release` nem em outros artefatos fora do escopo
//! do cache RTS.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Diretorios considerados cache de objects gerados pelo RTS.
fn cache_dirs() -> Vec<PathBuf> {
    vec![
        // Padrao atual — Etapa 5
        PathBuf::from("node_modules").join(".rts"),
        // Legado — pode coexistir em projetos que ainda nao migraram
        PathBuf::from("target").join(".deps"),
        PathBuf::from("target").join(".launcher"),
    ]
}

pub fn command() -> Result<()> {
    let mut removed_any = false;
    let mut freed_bytes = 0u64;

    for dir in cache_dirs() {
        if !dir.exists() {
            continue;
        }

        let (files, bytes) = count_files_and_bytes(&dir)?;
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove cache dir {}", dir.display()))?;

        println!(
            "removed {} ({} files, {} bytes)",
            dir.display(),
            files,
            bytes
        );

        removed_any = true;
        freed_bytes += bytes;
    }

    if !removed_any {
        println!("nothing to clean — no RTS cache directories found");
    } else {
        println!("freed {freed_bytes} bytes total");
    }

    Ok(())
}

/// Conta recursivamente arquivos e bytes em um diretorio.
/// Usado apenas para reportar ao usuario o que esta sendo removido.
fn count_files_and_bytes(dir: &Path) -> Result<(usize, u64)> {
    let mut files = 0usize;
    let mut bytes = 0u64;

    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let (sub_files, sub_bytes) = count_files_and_bytes(&path)?;
            files += sub_files;
            bytes += sub_bytes;
        } else if let Ok(metadata) = entry.metadata() {
            files += 1;
            bytes += metadata.len();
        }
    }

    Ok((files, bytes))
}
