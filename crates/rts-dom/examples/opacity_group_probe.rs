//! Quantos elementos de uma página REAL precisariam mesmo de um grupo de
//! compositing para o `opacity` sair certo.
//!
//! ## A pergunta, e porque só um número a responde
//!
//! O `opacity` deste motor multiplica o alpha das cores PRÓPRIAS de cada caixa,
//! uma a uma. Um grupo de compositing renderiza a subárvore inteira e só depois
//! a compõe com o fundo. As duas coisas dão **exatamente o mesmo pixel** exceto
//! onde duas camadas pintadas se SOBREPÕEM dentro do elemento: compor áreas
//! disjuntas dá o mesmo resultado nas duas ordens, e num elemento que pinta uma
//! coisa só não há segunda ordem nenhuma.
//!
//! Portanto a pergunta não é "quantos elementos têm `opacity`" — é "em quantos
//! é que ela cai sobre camadas sobrepostas". A contagem da folha de estilo não
//! sabe responder: metade das 210 declarações do corpus são `0` ou `1`, que não
//! precisam de grupo em caso nenhum, e as fracionárias do MediaWiki estão quase
//! todas em ícones, que pintam uma camada só.
//!
//! Esta sonda existe porque a decisão de abrir ou não a campanha do grupo de
//! compositing depende deste número, e ninguém o tinha.
//!
//! ## O que ela NÃO faz
//!
//! Não rasteriza, não mede diferença de cor por pixel e não diz se a diferença
//! seria VISÍVEL. Diz onde ela existe. Uma sobreposição de 0,1% da área de uma
//! caixa conta aqui como sobreposição — por isso o relatório traz também a área
//! sobreposta, que é o que separa "difere" de "difere o suficiente para alguém
//! reparar".
//!
//! ## Sobre a coluna AMBÍGUO
//!
//! Um elemento cai lá quando a sonda não consegue DECIDIR, e não quando o caso
//! é difícil: sem rect registado, não há como saber se duas camadas se cruzam.
//! Somá-los a qualquer das outras duas colunas daria um número inteiro que
//! ninguém pode verificar, e é justamente o que este trabalho não deve produzir.
//!
//!   cargo run -q -p rts-dom --example opacity_group_probe -- scripts/parity/pagina.combinada.html
//!   cargo run -q -p rts-dom --example opacity_group_probe -- google.html google.css

use rts_dom::layout::{self, Rect};
use rts_dom::{NodeIdx, NodeKind};

/// O medidor aproximado das outras sondas. Chega: o que se conta aqui é
/// SOBREPOSIÇÃO entre caixas, e a largura exata de um glifo não muda se duas
/// caixas se cruzam ou não.
struct Medidor;

impl layout::TextMeasurer for Medidor {
    fn text_width(&self, text: &str, size: f32, _mono: bool, _bold: bool, _italic: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }
    fn line_height(&self, size: f32) -> f32 {
        size * 1.3
    }
}

/// Uma camada pintada dentro da subárvore de um elemento.
struct Camada {
    rect: Option<Rect>,
    /// Texto: está sempre DENTRO da caixa do ancestral que o contém, portanto
    /// sobrepõe-se ao fundo dele sem ser preciso rect nenhum. É o que impede
    /// metade dos casos reais de cair em AMBÍGUO.
    texto: bool,
}

fn area_sobreposta(a: Rect, b: Rect) -> f32 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

/// O elemento pinta um FUNDO próprio (cor ou gradiente)?
fn pinta_fundo(css: &rts_dom::style::ComputedStyle) -> bool {
    css.bg.is_some() || css.gradient.is_some()
}

/// O elemento pinta uma BORDA visível? Conta como camada à parte do fundo
/// porque é desenhada POR CIMA dele, dentro da mesma caixa — quando os dois
/// existem, a sobreposição é certa sem olhar para rect nenhum.
fn pinta_borda(css: &rts_dom::style::ComputedStyle) -> bool {
    rts_dom::style::borders::resolved_sides(css).iter().any(|s| s.paints())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(caminho) = args.get(1) else {
        eprintln!("uso: opacity_group_probe <ficheiro.html> [folha.css]");
        std::process::exit(2);
    };
    let html = std::fs::read_to_string(caminho).expect("html");
    let mut dom = rts_dom::parse_html_to_dom(&html);
    if let Some(css) = args.get(2).and_then(|p| std::fs::read_to_string(p).ok()) {
        dom.add_stylesheet(&css);
    }
    let medidor = Medidor;
    let ctx = layout::LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &medidor };
    let lista = layout::layout_document(&dom, &ctx);

    let (mut com_opacity, mut zero_ou_um) = (0usize, 0usize);
    let (mut difere, mut igual, mut ambiguo) = (0usize, 0usize, 0usize);
    // A área sobreposta de cada elemento que DIFERE, como fração da caixa dele.
    // É o que separa "difere" de "difere o suficiente para se ver".
    let mut fracoes: Vec<f32> = Vec::new();
    let mut exemplos: Vec<String> = Vec::new();

    for idx in 0..dom.nodes.len() {
        let idx = idx as NodeIdx;
        if !matches!(dom.node(idx).kind, NodeKind::Element { .. }) {
            continue;
        }
        let Some(css) = dom.computed_style_idx(idx) else { continue };
        let Some(op) = css.opacity else { continue };
        com_opacity += 1;
        // `0` e `1` não precisam de grupo em caso nenhum: um é identidade, e a
        // zero nada é pintado nas duas formas. Contados à parte porque são
        // METADE das declarações da folha e enviesariam o total.
        if op <= 0.0 || op >= 1.0 {
            zero_ou_um += 1;
            continue;
        }

        let mut camadas: Vec<Camada> = Vec::new();
        let rect_proprio = lista.node_rects.get(&idx).copied();
        if pinta_fundo(&css) {
            camadas.push(Camada { rect: rect_proprio, texto: false });
        }
        if pinta_borda(&css) {
            camadas.push(Camada { rect: rect_proprio, texto: false });
        }
        // Descendentes: qualquer um que pinte é uma camada por cima.
        let mut pilha: Vec<NodeIdx> = dom.node(idx).children.clone();
        while let Some(d) = pilha.pop() {
            match &dom.node(d).kind {
                NodeKind::Text(t) if !t.trim().is_empty() => {
                    camadas.push(Camada { rect: None, texto: true });
                }
                NodeKind::Element { .. } => {
                    if let Some(dcss) = dom.computed_style_idx(d) {
                        if pinta_fundo(&dcss) || pinta_borda(&dcss) {
                            camadas.push(Camada { rect: lista.node_rects.get(&d).copied(), texto: false });
                        }
                    }
                    pilha.extend(dom.node(d).children.iter().copied());
                }
                _ => {}
            }
            // Uma subárvore de milhares de nós não muda a resposta depois da
            // segunda camada sobreposta; parar cedo mantém a sonda utilizável
            // numa página de 16 000 elementos.
            if camadas.len() > 64 {
                break;
            }
        }

        if camadas.len() < 2 {
            igual += 1;
            continue;
        }
        // TEXTO sobre FUNDO é sobreposição certa — o glifo é pintado dentro da
        // caixa que o contém. Não precisa de rect e não é ambíguo.
        let tem_fundo_proprio = pinta_fundo(&css) || pinta_borda(&css);
        if tem_fundo_proprio && camadas.iter().any(|c| c.texto) {
            difere += 1;
            fracoes.push(1.0);
            if exemplos.len() < 6 {
                exemplos.push(format!("<{}> opacity={op} fundo+texto", tag(&dom, idx)));
            }
            continue;
        }
        // Caso geral: precisa dos rects. Sem eles não se decide.
        let com_rect: Vec<Rect> = camadas.iter().filter_map(|c| c.rect).collect();
        let sem_rect = camadas.len() - com_rect.len();
        let mut maior = 0.0f32;
        for i in 0..com_rect.len() {
            for j in (i + 1)..com_rect.len() {
                maior = maior.max(area_sobreposta(com_rect[i], com_rect[j]));
            }
        }
        if maior > 0.0 {
            difere += 1;
            let base = rect_proprio.map(|r| r.w * r.h).filter(|a| *a > 0.0);
            fracoes.push(base.map(|a| (maior / a).min(1.0)).unwrap_or(1.0));
            if exemplos.len() < 6 {
                exemplos.push(format!("<{}> opacity={op} {} camadas", tag(&dom, idx), camadas.len()));
            }
        } else if sem_rect > 0 {
            // Havia camadas que não se conseguiu situar: a resposta seria um
            // palpite. Fica dito em vez de somado.
            ambiguo += 1;
        } else {
            igual += 1;
        }
    }

    fracoes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mediana = fracoes.get(fracoes.len() / 2).copied().unwrap_or(0.0);
    let grandes = fracoes.iter().filter(|f| **f >= 0.5).count();

    println!("ficheiro: {caminho}");
    println!("elementos com `opacity` declarada .... {com_opacity}");
    println!("  dos quais 0 ou 1 (grupo irrelevante)  {zero_ou_um}");
    println!("  fracionários ......................... {}", com_opacity - zero_ou_um);
    println!();
    println!("dos fracionários:");
    println!("  DIFERE  (camadas sobrepostas) ....... {difere}");
    println!("  IGUAL   (uma camada, ou disjuntas) .. {igual}");
    println!("  AMBÍGUO (sem rect p/ decidir) ....... {ambiguo}");
    println!();
    println!("sobreposição, como fração da caixa: mediana {:.0}%, com >=50% {grandes}", mediana * 100.0);
    if !exemplos.is_empty() {
        println!("exemplos:");
        for e in &exemplos {
            println!("  {e}");
        }
    }
}

fn tag(dom: &rts_dom::Dom, idx: NodeIdx) -> String {
    match &dom.node(idx).kind {
        NodeKind::Element { tag } => tag.clone(),
        _ => "?".into(),
    }
}
