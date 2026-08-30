//! A fila de timers de página — o `setTimeout`/`setInterval`/`requestAnimationFrame`
//! que um `<script>` agenda, dirigida pelo frame do host.
//!
//! # Por que em Rust, e não no prelude
//!
//! O mesmo motivo do [`crate::scope`]: cada `new Function` compila um PROGRAMA
//! novo. Uma fila em `.ts` de prelude seria por-programa — o script agendaria
//! na sua, e o laço do host, que é outro programa, bombearia uma fila vazia
//! para sempre.
//!
//! Isto também foi apagado com o motor antigo (`rts-dom/src/timerscope.rs`, em
//! `46910d997`), pela mesma razão e com o mesmo efeito: o `.ts` continuou a
//! chamar `DomTimers` e nada em Rust reparou.
//!
//! # O que mudou em relação ao que havia
//!
//! **O callback deixou de precisar de ser uma word tagueada à mão.** Metade da
//! doc do módulo anterior era sobre isso — uma função guardada como `f64`
//! voltava `undefined` e não-chamável. Aqui um nativo já troca `u64` opaco.
//!
//! **E é mantido vivo por [`rts_core::entry::hold_current`].** Um callback
//! agendado não está em frame nenhum entre o `setTimeout` e o disparo: se o
//! coletor correr no meio — e um `setInterval` de 16 ms garante que corre — o
//! único sítio que o nomeia é esta fila, que é memória do Rust e invisível ao
//! mark. Aqui o `external` é a ferramenta certa e não um desvio dela: os timers
//! VIVOS de uma página são mesmo o punhado que a sua doc descreve, ao contrário
//! do saco de globais.
//!
//! # O relógio é medido aqui
//!
//! O `.ts` não passa "agora": pergunta se há um timer vencido. Um por chamada,
//! porque o pump drena num laço com teto — assim um `setInterval(1)` atrasa-se
//! em vez de travar o frame.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rts_core::entry::{self, Provided};

use crate::value::{handle, int, integer, nothing};

struct Timer {
    id: i64,
    /// O identificador que mantém o callback vivo até disparar ou ser cancelado.
    hold: u32,
    deadline: Instant,
    /// `None` para um `setTimeout`; o período para um `setInterval`.
    period: Option<Duration>,
}

#[derive(Default)]
struct Queue {
    next_id: i64,
    timers: Vec<Timer>,
}

/// GLOBAL, como o saco de globais e pela mesma razão — ver [`crate::scope`].
/// O módulo anterior usava um `thread_local`; um `Mutex` global responde certo
/// em todos os casos em que aquele respondia, e também naquele em que não.
fn queues() -> &'static Mutex<HashMap<u64, Queue>> {
    static QUEUES: OnceLock<Mutex<HashMap<u64, Queue>>> = OnceLock::new();
    QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn locked() -> std::sync::MutexGuard<'static, HashMap<u64, Queue>> {
    match queues().lock() {
        Ok(queues) => queues,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Descarta os timers deste documento, soltando cada callback.
///
/// Chamado pelo `free` do documento. Os holds são recolhidos sob o lock e
/// soltos DEPOIS de o largar: `release_current` entra no runtime, e entrar no
/// runtime com este lock na mão é a forma de o próximo agendamento bloquear
/// contra nós.
pub fn drop_timers(h: u64) {
    let holds: Vec<u32> = locked()
        .remove(&h)
        .map(|queue| queue.timers.iter().map(|timer| timer.hold).collect())
        .unwrap_or_default();
    for hold in holds {
        entry::release_current(hold);
    }
}

pub const MEMBERS: &[(&str, Provided)] = &[
    ("add", add),
    ("cancel", cancel),
    ("takeDue", take_due),
    ("drop", drop_member),
];

/// `DomTimers.add(h, fn, ms, repeat)` — o id do timer.
///
/// O `hold` é tomado ANTES do lock, pela regra que o resto deste crate segue:
/// entrar no runtime é o que pode disparar uma coleção, e uma coleção não deve
/// encontrar este lock tomado.
extern "C" fn add(_e: u64, _t: u64, doc: u64, callback: u64, ms: u64, repeat: u64) -> u64 {
    let h = handle(doc);
    let delay = integer(ms, 0).max(0) as u64;
    let repeats = integer(repeat, 0) != 0;
    let hold = entry::hold_current(callback);
    let mut queues = locked();
    let queue = queues.entry(h).or_default();
    queue.next_id += 1;
    let id = queue.next_id;
    queue.timers.push(Timer {
        id,
        hold,
        deadline: Instant::now() + Duration::from_millis(delay),
        // Um `setInterval(f, 0)` que reagendasse para o mesmo instante seria
        // sempre o timer vencido, e o pump nunca chegaria aos outros. Um
        // milissegundo é o piso que o mantém no laço sem o deixar esfomear.
        period: repeats.then(|| Duration::from_millis(delay.max(1))),
    });
    int(id)
}

/// `DomTimers.cancel(h, id)` — descarta o timer, se existe.
extern "C" fn cancel(_e: u64, _t: u64, doc: u64, id: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let id = integer(id, -1);
    let hold = {
        let mut queues = locked();
        queues.get_mut(&h).and_then(|queue| {
            let at = queue.timers.iter().position(|timer| timer.id == id)?;
            Some(queue.timers.remove(at).hold)
        })
    };
    if let Some(hold) = hold {
        entry::release_current(hold);
    }
    nothing()
}

/// `DomTimers.takeDue(h)` — o callback de UM timer vencido, `undefined` se
/// nenhum venceu.
///
/// Um por chamada e não a lista toda: quem chama é o pump, que drena num laço
/// com teto. Devolver todos de uma vez tiraria dele a capacidade de parar a
/// meio de uma página que agenda mais depressa do que o frame corre.
///
/// Um `setInterval` é reagendado a partir de AGORA e não do prazo anterior. A
/// diferença aparece quando o pump se atrasa: acumular a partir do prazo faria
/// um intervalo atrasado disparar várias vezes seguidas para "recuperar", que é
/// exatamente o que trava o frame que ele já estava a atrasar.
extern "C" fn take_due(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let now = Instant::now();
    // O que a fila decide sob o lock: qual callback sai, e se o hold dele morre
    // com a saída. Tudo o que entra no runtime acontece depois de o largar.
    let taken: Option<(u32, bool)> = {
        let mut queues = locked();
        queues.get_mut(&h).and_then(|queue| {
            let at = queue
                .timers
                .iter()
                .position(|timer| timer.deadline <= now)?;
            match queue.timers[at].period {
                Some(period) => {
                    queue.timers[at].deadline = now + period;
                    Some((queue.timers[at].hold, false))
                }
                None => Some((queue.timers.remove(at).hold, true)),
            }
        })
    };
    let Some((hold, done)) = taken else {
        return nothing();
    };
    let callback = entry::held_current(hold).unwrap_or_else(nothing);
    if done {
        entry::release_current(hold);
    }
    callback
}

/// `DomTimers.drop(h)` — descarta os timers deste documento.
extern "C" fn drop_member(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    drop_timers(handle(doc));
    nothing()
}
