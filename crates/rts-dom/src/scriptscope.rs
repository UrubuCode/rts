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

use std::cell::RefCell;
use std::collections::HashMap;

use rts_engine::abi::ty::{Handle, Poly};

#[derive(Default)]
struct Scope {
    globals: HashMap<String, u64>,
    order: Vec<String>,
}

thread_local! {
    static SCOPES: RefCell<HashMap<u64, Scope>> = RefCell::new(HashMap::new());
}

fn s_count(h: u64) -> i64 {
    SCOPES.with(|s| s.borrow().get(&h).map(|x| x.order.len() as i64).unwrap_or(0))
}

fn s_name_at(h: u64, n: i64) -> String {
    if n < 0 {
        return String::new();
    }
    SCOPES.with(|s| {
        s.borrow()
            .get(&h)
            .and_then(|x| x.order.get(n as usize).cloned())
            .unwrap_or_default()
    })
}

fn s_get(h: u64, name: &str) -> u64 {
    SCOPES.with(|s| {
        s.borrow()
            .get(&h)
            .and_then(|x| x.globals.get(name).copied())
            .unwrap_or(0)
    })
}

fn s_set(h: u64, name: &str, w: u64) {
    SCOPES.with(|s| {
        let mut s = s.borrow_mut();
        let sc = s.entry(h).or_default();
        if !sc.globals.contains_key(name) {
            sc.order.push(name.to_string());
        }
        sc.globals.insert(name.to_string(), w);
    });
}

fn s_has(h: u64, name: &str) -> i64 {
    SCOPES.with(|s| {
        s.borrow()
            .get(&h)
            .map(|x| i64::from(x.globals.contains_key(name)))
            .unwrap_or(0)
    })
}

fn s_drop(h: u64) {
    SCOPES.with(|s| {
        s.borrow_mut().remove(&h);
    });
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
    fn count(h: Handle) -> f64 {
        s_count(h) as f64
    }

    /// O n-ésimo nome publicado ("" fora de faixa).
    #[rtse::statical]
    fn nameAt(h: Handle, n: f64) -> String {
        s_name_at(h, n as i64)
    }

    /// O VALOR do global (`undefined` se não existe). `Poly` para atravessar
    /// sem coerção — com `f64` a função vira `undefined`.
    #[rtse::statical]
    fn get(h: Handle, name: &str) -> Poly {
        let w = s_get(h, name);
        if w == 0 {
            rts_engine::heap::poly::POLY_UNDEFINED
        } else {
            w
        }
    }

    /// Guarda o VALOR sob `name`.
    #[rtse::statical]
    fn set(h: Handle, name: &str, value: Poly) {
        s_set(h, name, value);
    }

    /// `1` se o global existe.
    #[rtse::statical]
    fn has(h: Handle, name: &str) -> f64 {
        s_has(h, name) as f64
    }

    /// Descarta o saco deste documento.
    #[rtse::statical]
    fn drop(h: Handle) {
        s_drop(h);
    }
}
