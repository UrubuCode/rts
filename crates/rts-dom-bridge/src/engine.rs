//! Ponte mínima para invocar callbacks JavaScript armazenados como words ABI.
//!
//! O DOM continua sem conhecer o runtime: este módulo vive no bridge, que já é
//! responsável por atravessar a fronteira `rts-core` ↔ `rts-dom`.

use rts_core::entry::{self, Provided};

use crate::value::{integer, nothing};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("invoke_cb", invoke_callback),
    ("run_event_loop", run_event_loop),
    ("take_error", take_error),
];

/// `engine.invoke_cb(callbackWord, argument, receiver)` — chama um callback
/// armazenado pelo DOM com um argumento. O callback cruza como número para a
/// fachada TypeScript, por isso é reconstituído a partir do payload inteiro
/// antes de `entry::call`.
///
/// # O `receiver`, e o que a sua ausência custava
///
/// A linguagem diz que um ouvinte de evento corre com `this` ligado ao nó em
/// que está registado — o mesmo valor que `event.currentTarget`. Aqui o
/// receptor era `undefined` sempre, e isso não é um detalhe de conformidade:
///
///     function eventProxy(e) { return this.l[e.type + false](e) }
///
/// é o despachante do Preact, e ele é o ÚNICO ouvinte que o Preact regista por
/// tipo de evento — a tabela `l` mora no nó e ele chega lá por `this`. Com
/// `this` a valer `undefined`, nenhum `onClick` de nenhum componente Preact
/// dispara, e nada diz porquê.
///
/// O argumento é opcional em vez de obrigatório porque os três despachos da
/// fachada passam-no e nada mais precisa: um chamador que o omita fica com o
/// comportamento anterior em vez de com um erro sobre um parâmetro que não
/// conhece.
extern "C" fn invoke_callback(
    _e: u64,
    _this: u64,
    callback: u64,
    argument: u64,
    receiver: u64,
    _a3: u64,
) -> u64 {
    let callback = integer(callback, 0) as u64;
    let undefined = entry::undefined_value();
    let receiver = match receiver == undefined {
        true => undefined,
        false => receiver,
    };
    entry::call(
        callback, receiver, argument, undefined, undefined, undefined,
    )
}

/// `engine.run_event_loop()` — fecha o task da página.
///
/// Drena o que os `<script>` enfileiraram. Sem isto, um `.then` ou um
/// `queueMicrotask` registado por um script ficava na fila para sempre: o
/// callback nunca acontecia e nada dizia porquê.
extern "C" fn run_event_loop(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // As duas metades do loop, e não só uma.
    //
    // `drain_microtasks` corre o que uma promessa deixou pendente. O que ele
    // NÃO corre são as *sources* — os timers do motor, os sockets, e a entrega
    // de um `MessageChannel`, que é por onde o scheduler do React 18 despacha o
    // seu trabalho.
    //
    // Faltar essa metade é o que separava uma página VIVA de uma parada: num
    // programa headless o `await` devolve o controlo ao host, que corre o loop
    // completo, e tudo avançava; numa JANELA não há `await` — há um frame que
    // chama isto — e o trabalho agendado ficava na fila para sempre.
    //
    // Medido com o Jogo da Vida em React: headless a geração subia a cada 120
    // ms, na janela ficava em zero, com os mesmos scripts e sem um erro.
    //
    // As sources primeiro: uma delas pode enfileirar a microtask que a seguir
    // se drena, e a ordem inversa deixaria essa microtask para a volta seguinte.
    entry::pump_sources();
    entry::drain_microtasks();
    nothing()
}

/// `engine.take_error()` — o erro que uma microtask deixou pendente, e limpa-o.
///
/// `undefined` quando não houve nenhum.
///
/// Existe porque um throw dentro de uma microtask não passa por nenhum
/// `try`/`catch` de `.ts`: viaja num canal lateral do motor, e quem quiser
/// isolar a página como o console de um browser faz — reportar e seguir — tem
/// de o consumir explicitamente. Não consumir é pior do que parece: o slot
/// continua marcado, e a próxima verificação lê o erro de outra pessoa como se
/// fosse seu.
extern "C" fn take_error(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if entry::thrown() == 0 {
        return nothing();
    }
    entry::take_thrown()
}
