//! `Storage` — `localStorage` / `sessionStorage` (Web Storage API), em Rust.
//!
//! ## Por que em Rust e não `.ts`
//!
//! A primeira implementação foi um prelude `.ts` (`rts-dom/src/window.ts`).
//! Medido: acrescentar as ~68 linhas ao prelude fazia
//! `tests/node_util_parseargs.test.ts` — um teste de `node:util`, sem NENHUMA
//! relação — travar (CPU parada = bloqueio, não laço). Bisseção binária
//! (68 → 18 → 11 → 2 linhas) isolou o gatilho em DUAS linhas:
//!
//! ```ts
//! const __lsStores: any[] = [];
//! const __lsPaths: string[] = [];
//! ```
//!
//! Dois arrays globais VAZIOS, que ninguém lia nem escrevia. Estado global de
//! módulo num prelude vira gcell, e gcell é estado do PROGRAMA — o mesmo
//! mecanismo que na mesma sessão fez `const Node = new NodeConstants()` vazar
//! entre programas. Aqui o estado mora na struct e no HandleTable: não há
//! global de prelude, e a classe inteira de problema desaparece.
//!
//! Segue o padrão vigente (`docs/engine/architecture.md` +
//! `docs/engine/architecture.md`): declarado com `#[rtse::class]`,
//! símbolo e assinatura DERIVADOS da declaração Rust, tabela bakeada pelo
//! `rts-symbol-baker`. Nada escrito à mão.
//!
//! ## Persistência: PICKLE, não texto
//!
//! `persistTo(path)` liga o storage a um arquivo; toda mutação regrava. O
//! formato é o pickle do RTS (`rts_engine::heap::pickle`), não texto:
//!
//! - guarda ESTRUTURA E TIPO — um valor com `\n` ou com o separador dentro
//!   quebraria um "k=v por linha", e isso é conteúdo comum;
//! - é o mesmo mecanismo do resto do projeto, sem um parser novo só aqui;
//! - produz um bloco de bytes OPACO, que é exatamente o que se cifra quando a
//!   criptografia entrar — o ponto de entrada é [`encode`]/[`decode`], hoje a
//!   identidade (bytes crus em disco).
//!
//! A API pública segue a spec (string-only): `setItem` coage a string e
//! `getItem` devolve `string | null`.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_entry, alloc_rtse, with_entry, Entry};
use rts_engine::heap::pickle;

/// Um storage Web: pares chave→valor em ordem de inserção (a spec expõe
/// `key(n)` por índice), opcionalmente ligados a um arquivo.
#[rtse::class("Storage")]
#[derive(Clone, Default)]
pub struct Storage {
    keys: Vec<String>,
    vals: Vec<String>,
    /// Arquivo ligado por `persistTo`; vazio = em-memória (o padrão).
    path: String,
}

#[rtse::class("Storage")]
impl Storage {
    /// `new Storage()` — vazio e em-memória.
    #[rtse::ctor]
    fn new() -> Self {
        Storage::default()
    }

    /// `storage.length` — quantos pares há.
    #[rtse::getter]
    fn length(self: &Storage) -> f64 {
        self.keys.len() as f64
    }

    /// `storage.getItem(key)` — o valor, ou `null` se a chave não existe.
    #[rtse::method]
    fn getItem(self: &Storage, key: &str) -> Option<String> {
        self.index_of(key).map(|i| self.vals[i].clone())
    }

    /// `storage.setItem(key, value)` — insere ou sobrescreve, e persiste.
    #[rtse::method]
    fn setItem(self: &mut Storage, key: &str, value: &str) {
        match self.index_of(key) {
            Some(i) => self.vals[i] = value.to_string(),
            None => {
                self.keys.push(key.to_string());
                self.vals.push(value.to_string());
            }
        }
        self.flush();
    }

    /// `storage.removeItem(key)` — remove o par (no-op se ausente) e persiste.
    #[rtse::method]
    fn removeItem(self: &mut Storage, key: &str) {
        if let Some(i) = self.index_of(key) {
            self.keys.remove(i);
            self.vals.remove(i);
            self.flush();
        }
    }

    /// `storage.clear()` — esvazia e persiste.
    #[rtse::method]
    fn clear(self: &mut Storage) {
        self.keys.clear();
        self.vals.clear();
        self.flush();
    }

    /// `storage.key(n)` — a n-ésima chave em ordem de inserção, ou `null`.
    #[rtse::method]
    fn key(self: &Storage, n: f64) -> Option<String> {
        if n < 0.0 {
            return None;
        }
        self.keys.get(n as usize).cloned()
    }

    /// Liga este storage a um ARQUIVO e carrega o que já houver nele.
    ///
    /// Superfície do HOST, não da página: um site não escolhe onde seu
    /// `localStorage` mora — por isso não existe no `Storage` de um browser.
    #[rtse::method]
    fn persistTo(self: &mut Storage, path: &str) {
        self.path = path.to_string();
        self.load();
    }
}

impl Storage {
    /// Índice da chave, se existir.
    fn index_of(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    /// Regrava o storage no arquivo ligado. No-op quando não há arquivo — o
    /// caso padrão (em-memória).
    fn flush(&self) {
        if self.path.is_empty() {
            return;
        }
        if let Some(bytes) = self.to_pickle() {
            let _ = std::fs::write(&self.path, encode(&bytes));
        }
    }

    /// Recarrega do arquivo, substituindo o conteúdo em memória. Arquivo
    /// ausente/ilegível/corrompido deixa o storage como está em vez de
    /// derrubar a página — é dado de cache, nunca vale quebrar o boot por ele.
    fn load(&mut self) {
        if self.path.is_empty() {
            return;
        }
        let Ok(raw) = std::fs::read(&self.path) else { return };
        let Ok(word) = pickle::deserialize_value(&decode(&raw)) else { return };
        if let Some((keys, vals)) = read_pair(word) {
            self.keys = keys;
            self.vals = vals;
        }
    }

    /// Serializa `[keys, vals]` como um grafo de valores do RTS.
    fn to_pickle(&self) -> Option<Vec<u8>> {
        let pair = alloc_entry(Entry::vec(vec![
            string_array(&self.keys) as i64,
            string_array(&self.vals) as i64,
        ]));
        pickle::serialize_value(pair).ok()
    }
}

/// Aloca um `Vec` de strings do RTS e devolve seu handle.
fn string_array(items: &[String]) -> u64 {
    let words: Vec<i64> = items
        .iter()
        .map(|s| alloc_entry(Entry::String(s.as_bytes().to_vec())) as i64)
        .collect();
    alloc_entry(Entry::vec(words))
}

/// Lê de volta o `[keys, vals]` produzido por [`Storage::to_pickle`].
/// `None` quando o stream não tem essa forma (arquivo de outra origem).
fn read_pair(word: u64) -> Option<(Vec<String>, Vec<String>)> {
    let (kw, vw) = with_entry(word, |e| match e {
        Some(Entry::Vec(v)) if v.len() == 2 => Some((v[0] as u64, v[1] as u64)),
        _ => None,
    })?;
    Some((read_string_array(kw)?, read_string_array(vw)?))
}

/// Lê um `Vec` de strings do RTS para `Vec<String>`.
fn read_string_array(handle: u64) -> Option<Vec<String>> {
    // COPIA as words ANTES de tocar em cada uma: `with_entry` segura o lock do
    // shard enquanto roda o corpo, e ler outro handle lá dentro pode cair no
    // MESMO shard — auto-deadlock. Soltar o lock primeiro é o padrão correto.
    let words: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => Some(v.to_owned_vec()),
        _ => None,
    })?;
    let mut out = Vec::with_capacity(words.len());
    for w in words {
        let s = with_entry(w as u64, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        })?;
        out.push(s);
    }
    Some(out)
}

/// Bytes do pickle → bytes do arquivo. Identidade hoje; é AQUI que a
/// criptografia entra quando for a hora (cifrar antes de gravar e decifrar em
/// [`decode`]), sem tocar em mais nada do storage.
fn encode(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

/// Bytes do arquivo → bytes do pickle.
fn decode(raw: &[u8]) -> Vec<u8> {
    raw.to_vec()
}

/// Um `Storage` novo e vazio, como handle — o que o host usa para montar o
/// `localStorage`/`sessionStorage` de um documento.
pub fn new_storage() -> Handle {
    alloc_rtse("Storage", Storage::default())
}
