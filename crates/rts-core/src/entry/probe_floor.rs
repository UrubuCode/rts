//! O PISO de criar um valor, para pôr um número no modelo antes de o construir.
//!
//! # Porque isto existe
//!
//! P8: *"uma linguagem de máquina tem resposta para 'quão rápido isto poderia
//! ser': as instruções que são necessárias. Logo uma mudança justificada por
//! velocidade declara o piso, o número medido, e a diferença."*
//!
//! O modelo que Go, .NET e Rust usam para uma função é **{endereço de código,
//! capturas}** e nada mais: os metadados (nome, aridade) vivem numa tabela
//! estática indexada por endereço — o pclntab do Go, as tabelas de metadata do
//! .NET — e nunca são campos da instância. `closure_new` aqui faz muito mais:
//! `described_at`, `callable_template`, `light_codes::insert`,
//! `mark_callable`, duas escritas de propriedade, `external::hold`/`release` e
//! `record_callable_template`.
//!
//! A pergunta que decide se vale a pena reescrever isso é: **quanto custaria o
//! modelo de duas palavras?** Estas funções respondem medindo-o, em vez de o
//! estimar — e elas NÃO são o modelo, são o piso dele. Deliberadamente não
//! fazem nada que a correcção exija, o que é a razão de estarem aqui e não em
//! `functions.rs`.
//!
//! # O que estas funções deliberadamente NÃO fazem
//!
//! O que elas devolvem **não é um closure utilizável**: nada o marca chamável,
//! nada o descreve, nada o mantém vivo através de uma coleta. São uma medição
//! do custo de alocar e escrever, nada mais, e qualquer uso delas fora de
//! `examples/entry_probe` é um erro. É por isso que vivem num módulo com este
//! nome em vez de parecerem uma alternativa a `closure_new`.
//!
//! Também não são um `#[rtse::entry]`: não têm porta, não são alcançáveis por
//! código compilado, e a sonda chama-as de Rust. O custo da porta já está
//! medido em `docs/codegen/entry-tax.md` (~5 ns, e 3,0 ns de ponta a ponta para
//! `NumberRemainder`), logo somar a porta a estes números é uma adição e não
//! outra medição.

use super::{Context, with_current};
use crate::value::Value;

/// Só a célula: o que `region.alloc` custa visto de onde um entry point está.
///
/// A linha de base das outras duas. Usa o layout que o runtime declara
/// primeiro, porque o que está a ser medido é o heap e não a forma.
pub fn probe_cell_only() -> u64 {
    with_current(|context| {
        let ty = u32::from(context.text_type_index());
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
        Value::from_slot(cell).bits()
    })
}

/// A célula mais duas palavras nos seus próprios slots.
///
/// **Este é o piso do modelo de máquina**: um valor de função em Go é um
/// ponteiro para `{code, captures}`, e um delegate em .NET é um objecto com
/// `_target` e `_methodPtr`. Se este número for pequeno, a diferença entre ele
/// e `closure_new` é o que a reescrita vale; se for grande, o modelo não é o
/// problema e a reescrita seria desperdício.
pub fn probe_two_words(code: i64, environment: u64) -> u64 {
    with_current(|context| {
        let ty = u32::from(context.text_type_index());
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
        // Através de `set_field` porque é o que um slot de célula custa a
        // escrever pela via que o runtime tem; um `payload_window` seria mais
        // barato e mediria uma coisa que `closure_new` não faz.
        context.region.set_field(cell, 0, code as u64);
        context.region.set_field(cell, 1, environment);
        Value::from_slot(cell).bits()
    })
}

/// O mesmo, mais a escrita na tabela lateral `callables` que `mark_callable`
/// faz.
///
/// Separada de [`probe_two_words`] para responder à pergunta do P3 com um
/// número: se o estado do callable viver na CÉLULA em vez de numa `Aside`,
/// quanto é que isso poupa? A diferença entre as duas é essa resposta.
pub fn probe_two_words_and_table(code: i64, environment: u64) -> u64 {
    let made = probe_two_words(code, environment);
    with_current(|context| {
        if let Some(cell) = Value(made).as_slot() {
            context.mark_callable(cell, code as u64, environment);
        }
        made
    })
}

/// Quanto custa alcançar o contexto e mais nada.
///
/// O chão de todas as outras: uma linha que não aloca nem escreve, só toma o
/// `with_current`. Sem isto, as três acima não têm de onde ser subtraídas, e
/// `docs/codegen/entry-tax.md` diz que o contexto não é o custo — isto é o que
/// confirma ou refuta essa frase para ESTE caminho em vez de para
/// `get_indexed`.
pub fn probe_context_only() -> u64 {
    with_current(|context: &mut Context| u64::from(context.text_type_index()))
}
