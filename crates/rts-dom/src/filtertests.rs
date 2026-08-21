//! `filter` e `clip-path` do estilo até à DISPLAY LIST.
//!
//! Os testes de `painteffects` fixam a aritmética; estes fixam a LIGAÇÃO, que é
//! onde os dois defeitos plausíveis vivem: uma cor emitida sem passar pela
//! matriz (e as cores da caixa saem por quatro pontos diferentes — sombra,
//! gradiente, fundo, borda), e um recorte que abre no índice errado.
//!
//! Vale dizer o que estes testes NÃO conseguem ver: se o pixel final na janela
//! é o certo. A display list é onde a decisão é tomada, portanto é aqui que se
//! pina; a confirmação visual precisa de uma captura.

use crate::layout::DisplayItem;
use crate::table::tests::geometria;

/// Todos os itens pintados, já materializados.
fn itens(html: &str) -> Vec<DisplayItem> {
    let (_, l) = geometria(html, 600.0);
    l.materialized().to_vec()
}

/// A cor do fundo da caixa de largura `w`.
///
/// Escolhe pela LARGURA e não por "a primeira que não é branca", que foi a
/// primeira versão e estava errada de um modo que só um teste de `invert`
/// apanha: `filter:invert(1)` sobre preto dá branco, e o helper descartava
/// justamente o resultado que ia verificar.
fn fundo_w(html: &str, w: f32) -> u32 {
    itens(html)
        .iter()
        .find_map(|i| match i {
            DisplayItem::SolidRect { color, rect, .. } if rect.w == w => Some(*color),
            _ => None,
        })
        .expect("o fundo devia ter sido pintado")
}

/// O fundo de uma caixa de 40px — a medida das amostras aqui.
fn fundo(html: &str) -> u32 {
    fundo_w(html, 40.0)
}

/// O caso de longe mais comum do corpus (`pagina.css`: 18 declarações de
/// `invert`): o fundo chega à lista já invertido.
#[test]
fn invert_chega_ao_fundo_da_caixa() {
    let c = fundo("<div style='background:#000000; filter:invert(1); width:40px; height:40px'>x</div>");
    assert_eq!(c, 0xFFFF_FFFF, "preto invertido é branco");
}

/// O `-webkit-filter` da folha real vale o mesmo que o nome padrão. Se o braço
/// do parse só reconhecesse um, este era o teste que o dizia.
#[test]
fn o_prefixo_webkit_pinta_igual() {
    let padrao = fundo_w("<div style='background:#336699; filter:grayscale(1); width:9px; height:9px'>x</div>", 9.0);
    let webkit = fundo_w("<div style='background:#336699; -webkit-filter:grayscale(1); width:9px; height:9px'>x</div>", 9.0);
    assert_eq!(padrao, webkit);
}

/// A BORDA sai por outro caminho que o fundo (`border_items`), e foi o ponto
/// que quase ficou de fora: a função é chamada também só para CONTAR itens, e
/// nessa chamada a matriz é a identidade de propósito.
#[test]
fn a_borda_tambem_e_filtrada() {
    let html = "<div style='background:#ffffff; border:2px solid #000000; filter:invert(1); width:40px; height:40px'>x</div>";
    // A borda UNIFORME sai como `Border` e a borda por lado como `SolidRect`;
    // olhar só para uma das variantes deixaria metade dos casos por verificar.
    let cores: Vec<u32> = itens(html)
        .iter()
        .filter_map(|i| match i {
            DisplayItem::SolidRect { color, .. } | DisplayItem::Border { color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    assert!(cores.contains(&0xFFFF_FFFF), "a borda preta invertida: {cores:02X?}");
    assert!(cores.contains(&0x0000_00FF), "o fundo branco invertido: {cores:02X?}");
}

/// A regra da casa, ao nível da lista: uma cadeia com `blur` deixa a caixa
/// EXATAMENTE como estava. Não é "quase igual" — é o mesmo `u32`.
#[test]
fn uma_cadeia_com_blur_nao_muda_um_pixel() {
    let sem = fundo("<div style='background:#336699; width:40px; height:40px'>x</div>");
    let com = fundo(
        "<div style='background:#336699; filter:blur(4px) brightness(1.5); width:40px; height:40px'>x</div>",
    );
    assert_eq!(sem, com, "o brightness da cadeia não pode ser aplicado sozinho");
}

/// `filter` numa caixa sem filtro nenhum não pode mexer em nada — a condição
/// que o lote não podia quebrar, e a que apanharia uma matriz mal composta a
/// escorregar para o caso comum.
#[test]
fn uma_caixa_sem_filter_pinta_o_que_pintava() {
    assert_eq!(
        fundo("<div style='background:#336699; width:40px; height:40px'>x</div>"),
        0x3366_99FF,
    );
}

/// `clip-path: inset()` abre um `BeginClip` no rect certo, e o rect é relativo
/// à mesma origem da caixa.
#[test]
fn inset_emite_um_clip_no_rect_certo() {
    let html = "<div style='background:#336699; clip-path:inset(10px); width:100px; height:60px'>x</div>";
    let clip = itens(html)
        .into_iter()
        .find_map(|i| match i {
            DisplayItem::BeginClip { rect, .. } => Some(rect),
            _ => None,
        })
        .expect("o inset devia ter aberto um clip");
    assert_eq!((clip.w, clip.h), (80.0, 40.0), "100-20 e 60-20: {clip:?}");
}

/// O clip do `clip-path` abre ANTES do fundo, ao contrário do de overflow, que
/// recorta só os filhos. É a diferença que faz o `clip-path` recortar também a
/// caixa — e trocá-la deixaria o fundo a transbordar sem que a largura do clip
/// denunciasse nada.
#[test]
fn o_clip_abre_antes_do_fundo_da_caixa() {
    let html = "<div style='background:#336699; clip-path:inset(5px); width:100px; height:60px'>x</div>";
    let itens = itens(html);
    let clip = itens.iter().position(|i| matches!(i, DisplayItem::BeginClip { .. })).unwrap();
    let bg = itens
        .iter()
        .position(|i| matches!(i, DisplayItem::SolidRect { color, .. } if *color == 0x3366_99FF))
        .expect("fundo");
    assert!(clip < bg, "clip em {clip}, fundo em {bg}");
}

/// As formas que não sabemos desenhar não abrem clip NENHUM. Recortar um
/// `polygon()` pela envolvente daria um retângulo com aparência de forma.
#[test]
fn polygon_nao_abre_clip() {
    let html = "<div style='background:#336699; clip-path:polygon(50% 0%, 100% 100%, 0% 100%); width:100px; height:60px'>x</div>";
    assert!(
        !itens(html).iter().any(|i| matches!(i, DisplayItem::BeginClip { .. })),
        "um polygon não deve recortar",
    );
}

/// Cada `BeginClip` tem de ter o seu `EndClip`: um clip que abre e não fecha faz
/// desaparecer tudo o que vem depois na página, e não só este elemento.
#[test]
fn os_clips_fecham_todos() {
    let html = "<div style='clip-path:inset(4px); overflow:hidden; width:100px; height:60px'>\
                <div style='background:#111111; height:200px'>x</div></div>";
    let mut nivel = 0i32;
    for i in itens(html) {
        match i {
            DisplayItem::BeginClip { .. } => nivel += 1,
            DisplayItem::EndClip { .. } => {
                nivel -= 1;
                assert!(nivel >= 0, "um EndClip a mais");
            }
            _ => {}
        }
    }
    assert_eq!(nivel, 0, "sobrou clip por fechar");
}
