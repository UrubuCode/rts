//! Manter vivos os callbacks que o DOM guarda — a raiz que faltava.
//!
//! # O defeito que isto fecha
//!
//! `Dom` guarda cada handler como um `i64` opaco (`ListenerRecord::callback`).
//! O coletor deste motor acha o que está vivo por duas vias: o que o `Context`
//! segura e uma varredura CONSERVATIVA da pilha da máquina. Um inteiro dentro
//! de um `HashMap` do Rust não é nenhuma das duas — `roots::scan_stack`
//! procura palavras que sejam referências CODIFICADAS, e um índice cru não o
//! é. Logo a célula do closure é varrida enquanto o listener continua
//! registado, e a invocação seguinte (`engine.invoke_cb`) encontra o que ficou
//! no lugar.
//!
//! O sintoma é `TypeError: object is not a function` — a mesma assinatura que
//! `rts_core::entry::functions` documenta para uma raiz perdida — num
//! `addEventListener` que NUNCA foi removido, sobre um elemento que continua
//! na árvore. Não é raro nem exótico: chega assim que houver alocação
//! suficiente para o coletor correr, o que numa página com um relógio são
//! segundos. Reproduzido em 25 linhas sem React e sem janela: registar um
//! listener, alocar 400 000 objectos, despachar o evento.
//!
//! É a terceira ocorrência da classe descrita em `docs/engine/lost-roots.md`,
//! e a que esse documento previu por escrito: *cada tabela lateral nova é uma
//! hipótese nova de faltar na lista*. Esta tabela foi escrita depois do aviso.
//!
//! # Porque uma RECONCILIAÇÃO e não um par take/release
//!
//! O óbvio seria segurar no `addEventListener` e largar no
//! `removeEventListener`. Não chega, porque nem todas as remoções passam pela
//! ponte: `once` remove o listener a meio do próprio despacho, dentro do
//! `rts-dom`, e `removeEventListener(type)` apaga vários de uma vez. Um
//! par simétrico deixaria posses penduradas — um vazamento silencioso, que é
//! só o outro lado da moeda do defeito que se está a corrigir.
//!
//! Então em vez de contabilizar transições, compara-se ESTADOS: depois de
//! qualquer operação que possa mexer na lista, pergunta-se ao documento que
//! words tem, segura-se o que é novo e larga-se o que desapareceu. É O(n) sobre
//! os listeners de um documento — dezenas, não milhares — e é robusto a
//! remoções que a ponte não vê, que é a propriedade que interessa.

use std::cell::RefCell;
use std::collections::HashMap;

use rts_core::entry::external;

thread_local! {
    /// Por documento, o que já está seguro: word do callback → identificador da
    /// posse. Por documento e não global porque `free(doc)` tem de poder largar
    /// tudo o que era só dele sem tocar noutro que continue vivo.
    static POSSE: RefCell<HashMap<u64, HashMap<i64, u32>>> = RefCell::new(HashMap::new());
}

/// Põe as posses deste documento de acordo com os listeners que ele tem agora.
///
/// Chamada depois de cada operação que possa acrescentar ou tirar um listener,
/// incluindo o despacho — é lá que os `once` se removem sozinhos.
pub fn sincroniza(h: u64) {
    let Some(mut vivos) = rts_dom::store::with_dom(h, |d| {
        // Os do lote em curso entram TAMBÉM: um `once` já saiu da lista de
        // listeners quando foi coletado, e ainda vai ser invocado. Largá-lo
        // aqui seria recriar o defeito exactamente na janela mais curta que
        // ele tem.
        let mut words = d.callback_words();
        for word in d.pending_dispatch_words() {
            if !words.contains(&word) {
                words.push(word);
            }
        }
        words
    }) else {
        // O documento já não existe: larga tudo o que era dele.
        liberta_documento(h);
        return;
    };
    POSSE.with(|posse| {
        let mut posse = posse.borrow_mut();
        let deste = posse.entry(h).or_default();
        vivos.sort_unstable();
        for word in &vivos {
            if !deste.contains_key(word) {
                // `hold` guarda os BITS do valor, que é o que o word já é.
                // `hold` guarda os BITS do valor, que é o que o word já é.
                deste.insert(*word, external::hold_current(*word as u64));
            }
        }
        deste.retain(|word, id| {
            if vivos.binary_search(word).is_ok() {
                true
            } else {
                external::release_current(*id);
                false
            }
        });
        if deste.is_empty() {
            posse.remove(&h);
        }
    });
}

/// Larga tudo o que este documento segurava. `free(doc)` chama isto.
///
/// Sem esta linha o defeito trocava de sinal: o documento morria e os handlers
/// dele ficavam vivos para sempre, o que numa aplicação que abre e fecha
/// páginas é um crescimento sem fim.
pub fn liberta_documento(h: u64) {
    POSSE.with(|posse| {
        if let Some(deste) = posse.borrow_mut().remove(&h) {
            for id in deste.values() {
                external::release_current(*id);
            }
        }
    });
}
