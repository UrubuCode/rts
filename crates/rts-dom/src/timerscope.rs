//! Fila de timers de página (`setTimeout`/`setInterval` dos `<script>`) — por
//! documento, dirigida pelo FRAME do host (`pumpTimerCallbacks` no `dom.ts`).
//!
//! Vive em Rust pelo mesmo motivo do saco de globais (`scriptscope.rs`): cada
//! `new Function` compila um PROGRAMA novo, então uma fila em `.ts` de prelude
//! seria por-programa — o script agendaria num programa e o loop do host, que é
//! outro programa, bombearia uma fila vazia para sempre. Aqui a fila é um
//! `thread_local` chaveado pelo handle do DOM, compartilhado entre programas.
//!
//! O callback é guardado como `Poly` (a word tagueada) — o único tipo que faz
//! uma FUNÇÃO atravessar a borda e voltar CHAMÁVEL (com `f64` vira `undefined`;
//! medido no `DomScope`, que guarda o `requireLazy` da Meta do mesmo jeito).
//!
//! O relógio é medido AQUI (`Instant` monotônico) — o `.ts` não passa "agora",
//! só pergunta "tem timer vencido?". Um por chamada: o pump drena num laço com
//! teto, então um `setInterval(1ms)` não trava o frame.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use rts_engine::abi::ty::{Handle, Poly};

struct Timer {
    id: i64,
    fn_word: u64,
    deadline: Instant,
    /// 0 = one-shot (`setTimeout`); >0 = período em ms (`setInterval`).
    interval_ms: u64,
}

#[derive(Default)]
struct TimerSet {
    next_id: i64,
    timers: Vec<Timer>,
}

thread_local! {
    static TIMERS: RefCell<HashMap<u64, TimerSet>> = RefCell::new(HashMap::new());
}

fn t_add(h: u64, fn_word: u64, ms: u64, repeat: bool) -> i64 {
    TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        let set = t.entry(h).or_default();
        set.next_id += 1;
        let id = set.next_id;
        set.timers.push(Timer {
            id,
            fn_word,
            deadline: Instant::now() + std::time::Duration::from_millis(ms),
            interval_ms: if repeat { ms.max(1) } else { 0 },
        });
        id
    })
}

fn t_cancel(h: u64, id: i64) {
    TIMERS.with(|t| {
        if let Some(set) = t.borrow_mut().get_mut(&h) {
            set.timers.retain(|x| x.id != id);
        }
    });
}

/// Um timer VENCIDO: devolve a fn dele (re-armando interval, removendo
/// one-shot), ou 0 se nada venceu.
fn t_take_due(h: u64) -> u64 {
    TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        let Some(set) = t.get_mut(&h) else { return 0 };
        let now = Instant::now();
        let Some(pos) = set.timers.iter().position(|x| x.deadline <= now) else {
            return 0;
        };
        if set.timers[pos].interval_ms > 0 {
            let period = std::time::Duration::from_millis(set.timers[pos].interval_ms);
            set.timers[pos].deadline = now + period;
            set.timers[pos].fn_word
        } else {
            set.timers.remove(pos).fn_word
        }
    })
}

fn t_count(h: u64) -> i64 {
    TIMERS.with(|t| t.borrow().get(&h).map(|s| s.timers.len() as i64).unwrap_or(0))
}

fn t_drop(h: u64) {
    TIMERS.with(|t| {
        t.borrow_mut().remove(&h);
    });
}

/// Portador da fila de timers por documento. Só membros estáticos.
///
/// Classe (e não `#[rtse::function]` livre) porque o `rts-symbol-baker` ainda
/// não escaneia funções livres — o símbolo não entraria na tabela.
#[rtse::class("DomTimers")]
#[derive(Clone, Default)]
pub struct DomTimers {}

#[rtse::class("DomTimers")]
impl DomTimers {
    /// Agenda `f` para daqui `ms` ms; `repeat != 0` re-arma (interval).
    /// Devolve o id (para `cancel`).
    #[rtse::statical]
    fn add(h: Handle, f: Poly, ms: f64, repeat: f64) -> f64 {
        let ms = if ms.is_finite() && ms > 0.0 { ms as u64 } else { 0 };
        t_add(h, f, ms, repeat != 0.0) as f64
    }

    /// Cancela o timer `id` deste documento (no-op se não existe).
    #[rtse::statical]
    fn cancel(h: Handle, id: f64) {
        t_cancel(h, id as i64);
    }

    /// A fn de UM timer vencido (interval re-armado, one-shot removido);
    /// `undefined` se nada venceu. Chamar num laço com teto por frame.
    #[rtse::statical]
    fn takeDue(h: Handle) -> Poly {
        let w = t_take_due(h);
        if w == 0 {
            rts_engine::heap::poly::POLY_UNDEFINED
        } else {
            w
        }
    }

    /// Quantos timers este documento tem pendentes (diagnóstico).
    #[rtse::statical]
    fn count(h: Handle) -> f64 {
        t_count(h) as f64
    }

    /// Descarta a fila deste documento.
    #[rtse::statical]
    fn drop(h: Handle) {
        t_drop(h);
    }
}
