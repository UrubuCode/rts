//! `readTextFile(caminho)` — o texto de um ficheiro local, lido pela ponte.
//!
//! O `loadResources` do `dom.ts` (a `<link rel="stylesheet">` local, o
//! `@import`, o `<script src>`) chamava `fs.read_text` e `fetch.fetchText`,
//! dois globais do MOTOR ANTIGO que o motor novo nunca teve — e por isso
//! nenhuma folha externa carregava fora dos testes (que inlinam o CSS), e o
//! `examples/view.ts` do README abria a página sem estilo. Ler AQUI e não em
//! TS é o mesmo argumento de `setImageFile`: é a ponte que tem o `std::fs`.
//! `http(s)` continua por fazer — não há busca síncrona de texto no motor novo
//! (dito no `dom.ts`).

use rts_core::entry::Provided;

use crate::value::{string, text};

pub const MEMBERS: &[(&str, Provided)] = &[("readTextFile", read_text_file)];

/// `readTextFile(caminho)` → o conteúdo, ou `""` se não existe / não é UTF-8
/// (a convenção tolerante que o `__readResource` do `dom.ts` já assumia).
extern "C" fn read_text_file(_e: u64, _t: u64, caminho: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let caminho = text(caminho);
    let caminho = caminho.strip_prefix("file://").unwrap_or(&caminho);
    string(&std::fs::read_to_string(caminho).unwrap_or_default())
}
