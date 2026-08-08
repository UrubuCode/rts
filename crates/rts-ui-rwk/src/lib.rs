//! `rts:egui` e `rts:input` para o motor novo.
//!
//! # O que este crate é
//!
//! A ponte entre a superfície de host do `rts-core-rwk` — namespaces, coerção,
//! valores — e a lógica de UI, que vive no `rts-egui`/`rts-input` em Rust comum
//! e não sabe que existe um motor.
//!
//! Nada de gráfico acontece aqui. Se algo neste crate abrir uma janela, medir um
//! texto ou decidir um layout, foi na direção errada: essa é a lógica, e ela
//! pertence ao crate que já a tem.
//!
//! # O que mudou em relação à superfície do motor antigo
//!
//! Três coisas, todas porque um nativo aqui é outra coisa que lá — um ponteiro
//! de função ao lado de uma célula, e não um símbolo de linker:
//!
//! | antes | agora | por quê |
//! |---|---|---|
//! | 13 argumentos posicionais | um objeto de opções | a convenção carrega 4 — `crate::value` |
//! | `meshUpload(win, ptr, n, ptr, m)` | views tipadas | o coletor move células; um endereço lido envelhece |
//! | `isOpen(): number` | `boolean` | existe um booleano de verdade aqui |
//!
//! O resto dos nomes é igual, de propósito: um programa que já desenha continua
//! a mesma leitura, e a diferença fica onde ela é real.
//!
//! # O que NÃO está aqui, e não por esquecimento
//!
//! **`rts:gpu`** — os buffers de compute SÃO handles do `HandleTable` do motor
//! antigo. Portá-los é decidir onde aqueles bytes vivem no novo, não escrever
//! uma casca. `drawWater`, seu único consumidor, fica junto.
//!
//! **`rts:dom` e `render.*`** — a árvore, o parser e o motor de layout são 18 mil
//! linhas no `rts-dom`, e nenhuma delas conhece motor: o porte é a mesma casca
//! que este crate é, feita para a superfície daquele. `egui.render(win, dom)`
//! está fora por consequência — sem o namespace do DOM não há handle para
//! passar, e registrá-lo daria uma função que desenha nada.
//!
//! Cada ausência é uma linha que falha no `import`, que é onde ela deve doer.
//! `rts-std-rwk` já recusou uma vez registrar um `rts:egui` de fachada, pela
//! mesma razão: uma UI que compila e não pinta é o modo de falhar que custa mais
//! tempo até ser entendido.

//! # Reuse-check
//!
//! Rodada tarde — depois de o crate existir, quando deveria ter sido antes de a
//! primeira linha ser escrita, que é o que a RULE 0b chama de "a que não é
//! opcional". Fica dito porque um resultado limpo obtido fora de hora não é o
//! mesmo que um obtido na hora, e o próximo leitor merece saber qual foi.
//!
//! O que ela encontrou:
//!
//! - **Coerção, texto, bytes, propriedade, valores**: tudo já existe em
//!   `rts_core_rwk::entry` e é CHAMADO — `number_of`, `string_in`, `bytes_of`,
//!   `get_member`, `make_number`/`boolean_value`/`make_string`. `value.rs` não
//!   converte nada por conta própria.
//! - **A única segunda resposta**: `value::integer`, cujo vizinho é
//!   `rts_core_rwk::value::to_int32`. Difere e o porquê está no doc dela.
//! - **Nenhuma numeração nova.** Os handles de janela e de malha são do
//!   `rts-egui` e já existiam; este crate só os transporta como número. Não há
//!   duas tabelas de um número, que é o caso fatal do §3 da skill.
//! - **Nenhuma tabela de estado.** O estado de UI vive no `UiCtx`; o de input,
//!   na fonte ativa. Este crate é sem estado.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod draw;
pub mod input;
pub mod scene;
pub mod value;
pub mod window;

use rts_core_rwk::entry::{self, Context, Provided};

/// Registra `rts:egui` e `rts:input`.
///
/// Chamado por um host antes de o programa rodar, e por ele e não por um
/// construtor daqui: quais módulos existem é uma decisão sobre o ambiente que o
/// programa recebe, e um crate que se registrasse sozinho a estaria tomando.
/// É a mesma assinatura do `rts_std_rwk::install`, pela mesma razão.
pub fn install(context: &mut Context) {
    // O egui como backend ATIVO de render e como FONTE ativa de input. Sem isto
    // `rts:input` responderia o default de tudo — mouse em `-1`, nenhuma tecla
    // — que é indistinguível de um programa cujo usuário não fez nada. Registrar
    // aqui e não ao abrir a janela: o registro é por THREAD, e é esta que vai
    // rodar o programa.
    rts_egui::render_backend::register_backend();

    let surface = namespace(context);
    entry::declare_module(context, "rts:egui", surface);

    let entry_points = entry::make_namespace(context, input::MEMBERS);
    entry::declare_module(context, "rts:input", entry_points);
}

/// Solta os recursos de GPU enquanto o processo ainda está inteiro.
///
/// # Por que isto não pode ficar para o destrutor do thread-local
///
/// No Windows o destrutor de um `thread_local` roda a partir do callback de TLS
/// durante o `LdrShutdownProcess` — isto é, ENQUANTO as DLLs do driver estão
/// sendo descarregadas. Destruir um `wgpu::Device` nesse momento faz o driver
/// D3D12 da AMD dar `__fastfail` (`0xC0000409`) depois de uma execução limpa e
/// já totalmente impressa, sem mensagem nenhuma.
///
/// O `rts-egui` já sabia disso e tinha o `shutdown_shared_gpu` para o CLI antigo
/// chamar. O motor novo precisa do mesmo, e por isto é o HOST que chama e não o
/// programa: um programa que esquecesse a linha morreria na saída, e ele não tem
/// como saber que essa linha existe.
///
/// Idempotente, e no-op quando nenhuma GPU foi criada.
pub fn shutdown() {
    rts_egui::shutdown_shared_gpu();
}

/// `rts:egui`, montado das três metades da superfície.
///
/// Um objeto só, e não três: um programa escreve `egui.beginFrame` e
/// `egui.drawMesh` sem saber que uma é janela e a outra é cena. A divisão em
/// módulos é de LEITURA — janela, desenho, cena mudam por razões diferentes — e
/// não deve vazar para quem chama.
fn namespace(context: &mut Context) -> u64 {
    let members: Vec<(&str, Provided)> = window::MEMBERS
        .iter()
        .chain(draw::MEMBERS)
        .chain(scene::MEMBERS)
        .copied()
        .collect();
    entry::make_namespace(context, &members)
}
