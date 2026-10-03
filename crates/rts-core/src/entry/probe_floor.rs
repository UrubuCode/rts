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

/// Duas palavras mais as CONSULTAS que `closure_new` faz por instância.
///
/// # A pergunta que isto responde
///
/// `described_at(code)`, `callable_template(at)` e `light_codes::insert(code)`
/// são todas função do ENDEREÇO DE CÓDIGO, que é constante: um laço que cria
/// trezentas mil closures da mesma função pede a mesma resposta trezentas mil
/// vezes. Isso é o oposto do que uma linguagem de máquina faz — o Go resolve o
/// nome por endereço no pclntab quando alguém pergunta, não quando o valor é
/// criado.
///
/// Se a diferença entre isto e [`probe_two_words`] for grande, um cache
/// monomórfico por endereço (que é o que um inline cache é) vale mais do que
/// mover o estado para a célula, e vale-o sem mudar nada de observável. Se for
/// pequena, o custo está noutro sítio e este caminho fecha-se.
///
/// Não inclui as escritas de `name`/`length` nem o `external::hold`: esses já
/// estão medidos por ablação a 10 ns cada em JavaScript, logo somá-los aqui
/// seria contar duas vezes.
pub fn probe_two_words_and_lookups(code: i64, environment: u64) -> u64 {
    let made = probe_two_words(code, environment);
    with_current(|context| {
        // Exactamente as três que `closure_new` faz, na mesma ordem, e os
        // resultados são consumidos por `black_box` para que nada seja
        // eliminado por não ser usado — uma sonda cujo corpo o compilador
        // apaga mede um laço vazio e parece rápida.
        let described = context.described_at(code as u64);
        let has_prototype = described.is_none_or(|(_, _, has, _)| has);
        let at = usize::from(has_prototype) | (usize::from(described.is_some()) << 1);
        let template = context.callable_template(at);
        std::hint::black_box((described.is_some(), template.is_some()));
        made
    })
}

/// O mesmo, mais o `light_codes::insert` que `closure_new_light` faz por
/// instância.
///
/// O último candidato por-instância que não tinha número. É um conjunto de
/// endereços, e o endereço é o mesmo em todas as instâncias da mesma função —
/// logo um laço insere a mesma chave repetidamente, e o emissor já sabe quais
/// funções são leves (`Ctx::light_functions`) mas o host não lhe passa isso.
/// Se isto custar, semear o conjunto uma vez é a correcção; se não, fecha-se.
pub fn probe_two_words_and_light(code: i64, environment: u64) -> u64 {
    let made = probe_two_words(code, environment);
    with_current(|context| context.light_codes.insert(code as u64));
    made
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
