//! `tests/css/claude-img-sem-tamanho-natural-em-flex.html`,
//! `tests/css/claude-flex-abspos-img-aspect-ratio.html` e
//! `tests/css/claude-img-aspect-ratio-sem-loader.html` contra o Blink
//! (Edge 152, 2026-09-04) — lote imagens-no-raster: um `<img>` sem uma
//! dimensão deriva a outra pela razão de aspeto (CSS2 §10.3.2/§10.6.2), e um
//! item flex esticado no eixo cruzado (`align-items: stretch`, o default)
//! transfere essa razão para a base do eixo principal em vez de usar o
//! tamanho natural dos pixels (Flexbox §9.2, "transferred size").

use crate::table::tests::{geometria_com, rect};

/// `claude-img-sem-tamanho-natural-em-flex`: um `<img>` SEM `width`/`height`,
/// item de `#dentro` (flex-row, `height:100px`), que é por sua vez item de
/// `#fora` (flex-row, `width:100px`) — pela razão 1:1 e o stretch do eixo
/// cruzado dos dois níveis, `#alvo` deve ocupar 100×100.
///
/// A fixture usa uma fonte SVG `data:` (razão 1:1, 20×20 no viewBox); este
/// motor não tem descodificador de SVG (só PNG, PLAN.md lote V-img) — corte
/// dito, não deste lote. Os pixels são alimentados DIRETAMENTE por
/// `set_pixel_data` com a MESMA razão 1:1 (20×20), o mesmo método que
/// `img_natural`/`img_com_pixels_no_documento` já usam para testar a
/// GEOMETRIA sem depender de um descodificador: a pergunta aqui é se o
/// stretch transfere a razão através de DOIS níveis de flex, não se o SVG
/// decodifica.
#[test]
fn flex_stretch_transfere_a_razao_atraves_de_dois_niveis() {
    let html = r#"<style>
  #fora { width: 100px; display: flex; border: 1px solid #000; }
  #dentro { display: flex; height: 100px; }
</style>
<div id="fora">
  <div id="dentro">
    <img id="alvo">
  </div>
</div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#alvo").unwrap();
        d.set_pixel_data(id, vec![0, 200, 0, 255].repeat(400), 20, 20);
    });
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#fora"), (0.0, 0.0, 102.0, 102.0), "declarado + 1px de borda por lado");
    assert_eq!(r("#dentro"), (1.0, 1.0, 100.0, 100.0), "encolhe ao alvo transferido, não ao natural (20×20)");
    assert_eq!(r("#alvo"), (1.0, 1.0, 100.0, 100.0), "100×100: a altura definite de #dentro transferida à largura pela razão 1:1");
}

/// `claude-flex-abspos-img-aspect-ratio`: `#inner` (flex-row,
/// `position:absolute;top:0;bottom:0`) tem altura DEFINITE por inset
/// (lote C, `posicionado.rs`); `#img` sem `width`/`height` estica a altura
/// (`align-items: stretch`) e deriva a largura pela razão 1:1 do PNG
/// `data:` de 1×1 — 160×160, não 0×0 nem 1×1 (o tamanho natural cru).
///
/// O PNG é o MESMO `data:` da fixture, descodificado pelo decodificador REAL
/// (`crate::imagem`, movido de `rts-dom-bridge` neste lote) — não um
/// `set_pixel_data` à mão: esta é a imagem que `claude-raster`/
/// `claude-paint-dump` também vão carregar antes do layout.
#[test]
fn abspos_esticado_deriva_a_largura_pela_razao_do_png_data_url() {
    let data_url = "data:image/png;base64,\
        iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
    let bytes = crate::imagem::bytes_da_data_url(data_url).expect("data: url");
    let (rgba, w, h) = crate::imagem::png::decodificar(&bytes).expect("png 1x1");
    assert_eq!((w, h), (1, 1), "a fixture depende de um PNG 1×1 (razão 1:1)");

    let html = r#"<style>
  #outer { display: flex; width: 320px; height: 160px; }
  #item { width: 100%; }
  #mid { position: relative; height: 100%; }
  #inner { display: flex; position: absolute; top: 0; bottom: 0; }
  img { display: block; }
</style>
<div id="outer"><div id="item"><div id="mid"><div id="inner"><img id="img"></div></div></div></div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#img").unwrap();
        d.set_pixel_data(id, rgba, w, h);
    });
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#outer"), (0.0, 0.0, 320.0, 160.0), "declarado");
    assert_eq!(r("#inner"), (0.0, 0.0, 160.0, 160.0), "shrink-to-fit da largura TRANSFERIDA, não do 1×1 natural");
    assert_eq!(r("#img"), (0.0, 0.0, 160.0, 160.0), "altura esticada (160) × razão 1:1 = largura 160");
}

/// `claude-img-aspect-ratio-sem-loader`: um `<img>` com só `width` (CSS) e um
/// `src` que não resolve neste corpus (sem `support/20x50-green.png` — o
/// mesmo que aconteceu na medição: o Edge também não encontrou o ficheiro, é
/// por isso que o número medido NÃO é `w×(50/20)=225`, a razão do PNG real).
///
/// O medido no Blink é `(90, 16)` — a altura de fallback do ÍCONE de imagem
/// PARTIDA do Chromium (sem `alt`, o HTML não declara nenhum). Esse número
/// não vem de CSS2 §10.3.2/§10.6.2 (cujo fallback, quando só uma dimensão é
/// dada e não há razão nem intrínseco, é 150px — nenhuma leitura da spec dá
/// 16) nem de nenhuma constante que este motor já calcule: é um ícone de
/// UA cujo tamanho não está documentado em lado nenhum consultável. **Corte
/// dito**: este teste fixa o que o motor FAZ hoje (0 — sem razão disponível,
/// a altura fica por derivar) e não o ícone do Chromium; a fixture entra em
/// `tests/css/esperado-a-falhar.txt`.
#[test]
fn imagem_sem_ficheiro_e_sem_alt_fica_sem_altura_derivada() {
    let html = r#"<style>#img1{width:90px;display:block}</style>
<img id="img1" src="support/20x50-green.png">"#;
    let (dom, list) = geometria_com(html, 1280.0, |_d| {});
    let r = rect(&dom, &list, "#img1", 0);
    assert_eq!((r.w, r.h), (90.0, 0.0), "largura do CSS; altura sem razão nenhuma para derivar (gap conhecido, ver o comentário acima)");
}

/// `flexbox-min-width-auto-002a` (WPT, retrabalho do lote): `min-width:auto`
/// (o inicial) num `<img width:100 height:30>` de razão 1:1, num flex-row
/// `width:0` (encolhe ao piso) — candidato (d) do §4.5 ("se há razão de
/// aspeto, o candidato de conteúdo é a largura DERIVADA da razão pela
/// restrição no outro eixo", aqui `height`), não a `width` DECLARADA (100).
/// `table::widths::min_content` devolvia 100 (a `replaced_inline_size`
/// normal, que honra as duas dimensões quando as duas estão fixas — certo
/// para a caixa REAL, errado para este piso) e o item nunca encolhia abaixo
/// dela; o achado só apareceu quando o PNG passou a carregar de verdade
/// (antes, mascarado, a régua de pintura nunca via a diferença).
#[test]
fn min_width_auto_deriva_da_razao_quando_a_altura_tambem_esta_declarada() {
    let html = r#"<style>
  .f { display: flex; width: 0px; }
  .f > * { border: 2px dotted purple; height: 30px; }
</style>
<div class="f"><img id="alvo" style="width: 100px"></div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#alvo").unwrap();
        d.set_pixel_data(id, vec![0, 0, 200, 255].repeat(256), 16, 16);
    });
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (34.0, 34.0), "encolhe ao piso 30(+4 de borda)=34, derivado da altura 30 pela razão 1:1 — não fica em 104 (100+4, a largura declarada)");
}

/// `flexbox-min-height-auto-002a` (WPT, retrabalho do lote): o mesmo
/// candidato (d), no eixo de COLUNA — `min-height:auto` deriva da LARGURA
/// (o eixo cruzado da coluna) pela razão, não fica na `height` declarada
/// (100). `coluna_shrink::min_main_auto` devolvia 0 quando `height` estava
/// declarado (o ramo "sem `height`, usa o natural" não se aplicava, e não
/// havia terceiro ramo) — o item encolhia quase a nada em vez de parar em 30.
#[test]
fn min_height_auto_deriva_da_razao_quando_a_largura_tambem_esta_declarada() {
    let html = r#"<style>
  .f { display: flex; flex-direction: column; height: 1px; float: left; }
  .f > * { border: 2px dotted purple; width: 30px; }
</style>
<div class="f"><img id="alvo" style="height: 100px"></div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#alvo").unwrap();
        d.set_pixel_data(id, vec![0, 0, 200, 255].repeat(256), 16, 16);
    });
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (34.0, 34.0), "encolhe ao piso 30(+4 de borda)=34, derivado da largura 30 pela razão 1:1 — não fica em 104 (100+4, a altura declarada)");
}

/// `flexbox-definite-cross-size-constrained-percentage` (WPT, retrabalho do
/// lote): um `<img>` sem NENHUMA dimensão, esticado por `align-items:
/// stretch` num flex-row cuja altura é uma PERCENTAGEM (25px, de um
/// ancestral de 50px) — o transferido (`replaced_transferido::transferido`)
/// já dava a base certa (25), mas `table::min_content` mede o `<img>` pelo
/// NATURAL dos pixels (60, sem saber nada de eixo cruzado) e
/// `com_piso_minimo` erguia o item de volta ao natural, anulando o
/// transferido. `base_e_altura_do_item` devolve agora se transferiu, e
/// `flex.rs` usa a própria base transferida como piso nesse caso.
#[test]
fn item_transferido_nao_e_erguido_de_volta_ao_natural_pelo_piso() {
    let html = r#"<style>
  #fora { height: 50px; }
  #dentro { height: 50%; display: flex; }
</style>
<div id="fora"><div id="dentro"><img id="alvo"></div></div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#alvo").unwrap();
        d.set_pixel_data(id, vec![0, 200, 0, 255].repeat(3600), 60, 60);
    });
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (25.0, 25.0), "25×25 (altura de 25 do flex, transferida pela razão 1:1) — não 60×60 (o natural dos pixels, se o piso automático o erguer de volta)");
}
