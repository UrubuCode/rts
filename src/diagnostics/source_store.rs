//! Armazenamento global de arquivos fonte para diagnosticos.
//!
//! Cada arquivo carregado pelo compilador recebe um `FileId` opaco.
//! Spans passam a carregar o `FileId` e o renderer de diagnosticos
//! consulta o store para obter path + texto e exibir snippets.
//!
//! O store e global (OnceLock + RwLock). Leituras sao frequentes
//! (uma por diagnostico renderizado); escritas sao raras (uma por
//! arquivo carregado no grafo de modulos).

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
    /// Indices do inicio de cada linha dentro de `text` (em bytes).
    /// Permite extrair a linha N em O(1) sem varrer a string inteira.
    pub line_starts: Vec<usize>,
}

impl SourceFile {
    fn new(path: PathBuf, text: String) -> Self {
        let mut line_starts = vec![0usize];
        for (index, byte) in text.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            path,
            text,
            line_starts,
        }
    }

    /// Retorna o trecho da linha `line` (1-based), sem o terminador `\n`.
    pub fn line_text(&self, line: usize) -> Option<&str> {
        if line == 0 {
            return None;
        }
        let idx = line - 1;
        let start = *self.line_starts.get(idx)?;
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.text.len());
        let slice = &self.text[start..end];
        Some(slice.strip_suffix('\n').unwrap_or(slice).trim_end_matches('\r'))
    }
}

#[derive(Default)]
struct SourceStore {
    files: Vec<SourceFile>,
}

fn store() -> &'static RwLock<SourceStore> {
    static STORE: OnceLock<RwLock<SourceStore>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(SourceStore::default()))
}

/// Registra um arquivo no store e devolve o `FileId`.
///
/// Se o mesmo `path` ja tiver sido registrado, retorna o id existente
/// e atualiza o texto (util quando o mesmo arquivo e reparseado numa
/// sessao long-lived de `rts run --watch`).
pub fn register(path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
    let path = path.into();
    let text = text.into();
    let mut guard = store().write().expect("source store poisoned");
    if let Some((idx, file)) = guard
        .files
        .iter_mut()
        .enumerate()
        .find(|(_, f)| f.path == path)
    {
        *file = SourceFile::new(path, text);
        return FileId(idx as u32);
    }
    let id = FileId(guard.files.len() as u32);
    guard.files.push(SourceFile::new(path, text));
    id
}

/// Registra um fonte "anonimo" (snippets de `-e/--eval`, testes) com um
/// rotulo descritivo. O rotulo aparece no diagnostico como se fosse um path.
pub fn register_anonymous(label: &str, text: impl Into<String>) -> FileId {
    register(PathBuf::from(format!("<{label}>")), text)
}

/// Executa `f` com referencia ao arquivo. Retorna `None` se o id
/// nao estiver registrado (nao deveria acontecer em caminho feliz).
pub fn with_file<R>(id: FileId, f: impl FnOnce(&SourceFile) -> R) -> Option<R> {
    let guard = store().read().expect("source store poisoned");
    guard.files.get(id.0 as usize).map(f)
}

/// Retorna o path (clonado) de um arquivo registrado.
pub fn path_of(id: FileId) -> Option<PathBuf> {
    with_file(id, |file| file.path.clone())
}

/// Retorna a linha `line` (1-based) do arquivo como String.
pub fn line_text(id: FileId, line: usize) -> Option<String> {
    with_file(id, |file| file.line_text(line).map(str::to_string))?
}

/// Limpa o store. Usado em testes para garantir isolamento.
#[cfg(test)]
pub fn reset() {
    let mut guard = store().write().expect("source store poisoned");
    guard.files.clear();
}

/// Procura um `FileId` ja registrado para o path informado.
pub fn lookup(path: &Path) -> Option<FileId> {
    let guard = store().read().expect("source store poisoned");
    guard
        .files
        .iter()
        .position(|f| f.path == path)
        .map(|idx| FileId(idx as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Testes usam paths unicos para evitar colisao no store compartilhado.
    // O store nunca e zerado em producao — testes tambem evitam reset() para
    // nao interferir entre si quando cargo roda em paralelo.

    #[test]
    fn registers_and_reads_line() {
        let id = register(
            "test_store_registers_and_reads_line.ts",
            "line1\nline2\nline3\n",
        );
        assert_eq!(line_text(id, 1).as_deref(), Some("line1"));
        assert_eq!(line_text(id, 2).as_deref(), Some("line2"));
        assert_eq!(line_text(id, 3).as_deref(), Some("line3"));
        // Apos 3 linhas + \n final, a linha 4 esta vazia (decorrente do \n).
        // Aceitamos Some("") ou None — depende de o texto ter ou nao \n final.
        let line_4 = line_text(id, 4);
        assert!(line_4.is_none() || line_4.as_deref() == Some(""));
        assert_eq!(line_text(id, 5), None);
    }

    #[test]
    fn register_twice_updates_text() {
        let id1 = register("test_store_register_twice.ts", "old");
        let id2 = register("test_store_register_twice.ts", "new");
        assert_eq!(id1, id2);
        assert_eq!(line_text(id1, 1).as_deref(), Some("new"));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let id = register("test_store_crlf.ts", "a\r\nb\r\n");
        assert_eq!(line_text(id, 1).as_deref(), Some("a"));
        assert_eq!(line_text(id, 2).as_deref(), Some("b"));
    }
}
