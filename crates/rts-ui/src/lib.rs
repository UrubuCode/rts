//! `rts:egui` e `rts:input` para o motor novo.
//!
//! # O que este crate é
//!
//! A ponte entre a superfície de host do `rts-core` — namespaces, coerção,
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
//! # DUAS codificações de cor, e isto é uma armadilha e não um desenho
//!
//! O caminho 2D e o caminho 3D desta mesma superfície leem uma cor de formas
//! DIFERENTES, e nada no nome das funções avisa:
//!
//! | caminho | funções | codificação | onde se lê |
//! |---|---|---|---|
//! | 2D | `drawRect`, `drawText`, `drawLine` | `0xRRGGBBAA` | `rts-egui/src/frame/render.rs` — `r = c >> 24` |
//! | 3D | `drawMesh` | `0xAARRGGBB` | `rts-egui/src/scene_api.rs` — `a = c >> 24` |
//!
//! Um programa que compõe UI sobre uma cena chama as duas no mesmo frame, com
//! constantes hexadecimais escritas à mão, e trocá-las não dá erro nenhum: dá
//! uma cor errada, que é o defeito que se atribui ao próprio desenho antes de se
//! suspeitar da superfície. `0xFF4FC3F7` é azul opaco numa e um verde
//! transparente na outra.
//!
//! Está DITO aqui em vez de corrigido porque unificar quebra todo programa que
//! já escreveu as constantes de um dos dois lados — inclusive os exemplos deste
//! repositório. A correção quer uma decisão e uma passagem por todos os
//! chamadores, não um commit de conveniência; enquanto ela não vem, isto é o
//! aviso que faltava.
//!
//! Encontrado ao portar o rasterizador 2D de um projeto externo, verificando a
//! codificação em vez de presumi-la.
//!
//! # O que NÃO está aqui, e não por esquecimento
//!
//! `rts:gpu` ESTAVA nesta lista, com a justificativa de que um buffer de compute
//! seria um handle do motor antigo. Era falso — o `wgpu::Buffer` vive num
//! `HashMap` do próprio `compute`, e o que atravessava era só o buffer de bytes
//! do programa, que `bytes_of`/`write_bytes` já respondem. Está portado.
//!
//! **`rts:dom` e `render.*`** — a árvore, o parser e o motor de layout são 18 mil
//! linhas no `rts-dom`, e nenhuma delas conhece motor: o porte é a mesma casca
//! que este crate é, feita para a superfície daquele. `egui.render(win, dom)`
//! está fora por consequência — sem o namespace do DOM não há handle para
//! passar, e registrá-lo daria uma função que desenha nada.
//!
//! Cada ausência é uma linha que falha no `import`, que é onde ela deve doer.
//! `rts-std` já recusou uma vez registrar um `rts:egui` de fachada, pela
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
//!   `rts_core::entry` e é CHAMADO — `number_of`, `string_in`, `bytes_of`,
//!   `get_member`, `make_number`/`boolean_value`/`make_string`. `value.rs` não
//!   converte nada por conta própria.
//! - **A única segunda resposta**: `value::integer`, cujo vizinho é
//!   `rts_core::value::to_int32`. Difere e o porquê está no doc dela.
//! - **Nenhuma numeração nova.** Os handles de janela e de malha são do
//!   `rts-egui` e já existiam; este crate só os transporta como número. Não há
//!   duas tabelas de um número, que é o caso fatal do §3 da skill.
//! - **Nenhuma tabela de estado.** O estado de UI vive no `UiCtx`; o de input,
//!   na fonte ativa. Este crate é sem estado.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod draw;
pub mod gpu;
pub mod input;
pub mod scene;
pub mod value;
pub mod window;

use rts_core::entry::{self, Context, Provided};

/// Registra `rts:egui` e `rts:input`.
///
/// Chamado por um host antes de o programa rodar, e por ele e não por um
/// construtor daqui: quais módulos existem é uma decisão sobre o ambiente que o
/// programa recebe, e um crate que se registrasse sozinho a estaria tomando.
/// É a mesma assinatura do `rts_std::install`, pela mesma razão.
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

    let compute = entry::make_namespace(context, gpu::MEMBERS);
    entry::declare_module(context, "rts:gpu", compute);
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
/// # A ordem importa
///
/// `compute::shutdown` solta pipelines, buffers e stagings E a cópia que aquele
/// contexto tem dos handles do device, e só então o device compartilhado. Chamar
/// direto o `shutdown_shared_gpu` deixava o contexto de compute vivo com uma
/// referência ao device, então o drop dele acontecia depois — no destrutor de
/// thread-local, que é exatamente o momento que isto existe para evitar. Foi
/// visto: o `examples/gpu` computava certo, imprimia tudo e morria na saída.
///
/// Idempotente, e no-op quando nenhuma GPU foi criada.
pub fn shutdown() {
    rts_egui::compute::shutdown();
    // Não é GPU, mas é o mesmo "o processo vai mesmo morrer agora": limpa o
    // medidor de texto que a última janela a pintar registou em
    // `rts_dom::layout::medidor_ativo`, para não sobreviver ao processo que o
    // registou.
    rts_egui::clear_active_measurer();
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
