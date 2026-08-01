//! Saco de globais de cada documento — o objeto global que todos os `<script>`
//! de uma página compartilham.
//!
//! Vive em Rust (num `thread_local` chaveado pelo handle do DOM) porque precisa
//! ser COMPARTILHADO ENTRE PROGRAMAS: cada `new Function` compila um programa
//! novo, então um saco escrito em `.ts` seria por-programa e o script N+1 não
//! veria o do script N. Guardá-lo em arrays globais de prelude é pior — vira
//! gcell (estado do programa) e já travou um teste sem relação nesta sessão.
//!
//! O valor guardado é `Poly` — a WORD TAGUEADA, o tipo que atravessa a borda
//! SEM coerção. Com `f64` o round-trip devolve `undefined`; com `Poly` uma
//! string volta string e uma função volta CHAMÁVEL (medido).
//!
//! O lado `.ts` expõe isto como um Proxy cujas traps `get`/`set` caem aqui.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rts_engine::abi::ty::Poly;

#[derive(Default)]
struct Scope {
    globals: HashMap<String, u64>,
    order: Vec<String>,
}

/// GLOBAL (não `thread_local`): a trap `set` do Proxy que escreve aqui roda num
/// contexto de execução próprio, e com `thread_local` a escrita caía num mapa
/// que o leitor de fora nunca via — `count` voltava 0 logo depois de um `set`
/// bem-sucedido. Foi o que fez o `__d` da Meta (registrador de módulos) sumir
/// entre o script que o define e os bundles que o chamam.
fn scopes() -> &'static Mutex<HashMap<u64, Scope>> {
    static SCOPES: OnceLock<Mutex<HashMap<u64, Scope>>> = OnceLock::new();
    SCOPES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// GC ROOT SOURCE (issue #2069). Os globais da página vivem AQUI, num container
/// Rust — o scanner conservativo varre pilha + gcells + microtasks, mas NÃO este
/// `HashMap`. Sem esta marcação, um `__d`/módulo referenciado só pelo saco de
/// globais é invisível ao mark, o sweep o libera, e o próximo `__G.x` que a
/// página lê dereferencia um slot já liberado → USE-AFTER-FREE (segfault sob
/// bundles grandes: um ciclo de GC no meio da execução liberava ~10 mil handles,
/// incluindo vivos).
///
/// Registrado em `rts-runtime` ao lado da fila de microtasks. Coleta as words sob
/// o lock e SOLTA antes de marcar: `mark_handle` aloca (caminha filhos) e pode
/// re-disparar o GC, que re-entraria neste marcador — segurar o lock aqui
/// deadlockaria (mesma razão do `mark_all` copiar os fns antes de chamar).
pub fn mark_scriptscope_roots() {
    let words: Vec<u64> = {
        let g = scopes().lock().unwrap_or_else(|e| e.into_inner());
        g.values()
            .flat_map(|scope| scope.globals.values().copied())
            .collect()
    };
    for w in words {
        rts_engine::heap::handles::mark_handle(w);
    }
}

fn s_count(h: u64) -> i64 {
    scopes()
        .lock()
        .map(|s| s.get(&h).map(|x| x.order.len() as i64).unwrap_or(0))
        .unwrap_or(0)
}

fn s_name_at(h: u64, n: i64) -> String {
    if n < 0 {
        return String::new();
    }
    scopes()
        .lock()
        .ok()
        .and_then(|s| s.get(&h).and_then(|x| x.order.get(n as usize).cloned()))
        .unwrap_or_default()
}

fn s_get(h: u64, name: &str) -> u64 {
    scopes()
        .lock()
        .ok()
        .and_then(|s| s.get(&h).and_then(|x| x.globals.get(name).copied()))
        .unwrap_or(0)
}

fn s_set(h: u64, name: &str, w: u64) {
    if let Ok(mut s) = scopes().lock() {
        let sc = s.entry(h).or_default();
        if !sc.globals.contains_key(name) {
            sc.order.push(name.to_string());
        }
        sc.globals.insert(name.to_string(), w);
    }
}

fn s_has(h: u64, name: &str) -> i64 {
    scopes()
        .lock()
        .map(|s| {
            s.get(&h)
                .map(|x| i64::from(x.globals.contains_key(name)))
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn s_drop(h: u64) {
    if let Ok(mut s) = scopes().lock() {
        s.remove(&h);
    }
}

/// Descarta o estado deste documento (chamado pelo `free` do DOM).
pub fn drop_scope(h: u64) {
    s_drop(h);
}

/// Portador do saco de globais por documento. Só membros estáticos.
///
/// Classe (e não `#[rtse::function]` livre) porque o `rts-symbol-baker` ainda
/// não escaneia funções livres — o símbolo não entraria na tabela.
#[rtse::class("DomScope")]
#[derive(Clone, Default)]
pub struct DomScope {}

#[rtse::class("DomScope")]
impl DomScope {
    /// Quantos globais este documento publicou.
    #[rtse::statical]
    fn count(h: f64) -> f64 {
        s_count(h as u64) as f64
    }

    /// O n-ésimo nome publicado ("" fora de faixa).
    #[rtse::statical]
    fn nameAt(h: f64, n: f64) -> String {
        s_name_at(h as u64, n as i64)
    }

    /// O VALOR do global (`undefined` se não existe). `Poly` para atravessar
    /// sem coerção — com `f64` a função vira `undefined`.
    #[rtse::statical]
    fn get(h: f64, name: &str) -> Poly {
        let w = s_get(h as u64, name);
        if w == 0 {
            rts_engine::heap::poly::POLY_UNDEFINED
        } else {
            w
        }
    }

    /// Guarda o VALOR sob `name`.
    #[rtse::statical]
    fn set(h: f64, name: &str, value: Poly) {
        s_set(h as u64, name, value);
    }

    /// `1` se o global existe.
    #[rtse::statical]
    fn has(h: f64, name: &str) -> f64 {
        s_has(h as u64, name) as f64
    }

    /// Descarta o saco deste documento.
    #[rtse::statical]
    fn drop(h: f64) {
        s_drop(h as u64);
    }
}
