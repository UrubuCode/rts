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
/// A fixture usa uma fonte SVG `data:` (razão 1:1, 20×20 no viewBox). Este
/// motor não RASTERIZA SVG (só PNG, PLAN.md lote V-img — pintar continua
/// mascarado, corte mantido) mas `Dom::image_dims` agora lê a dimensão
/// intrínseca direto do texto da `data:` URL (`width`/`height`/`viewBox` da
/// RAIZ do `<svg>`, retrabalho deste lote) sem descodificar nada — por isso
/// o `<img>` real da fixture, sem nenhum `set_pixel_data` a simular nada,
/// já basta para testar que o stretch transfere a razão através de DOIS
/// níveis de flex.
#[test]
fn flex_stretch_transfere_a_razao_atraves_de_dois_niveis() {
    let html = r#"<style>
  #fora { width: 100px; display: flex; border: 1px solid #000; }
  #dentro { display: flex; height: 100px; }
</style>
<div id="fora">
  <div id="dentro">
    <img id="alvo" src="data:image/svg+xml,<svg width='20' height='20' viewBox='0 0 20 20' xmlns='http://www.w3.org/2000/svg'><rect width='100%' height='100%' fill='green'/></svg>">
  </div>
</div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |_d| {});
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#fora"), (0.0, 0.0, 102.0, 102.0), "declarado + 1px de borda por lado");
    assert_eq!(r("#dentro"), (1.0, 1.0, 100.0, 100.0), "encolhe ao alvo transferido, não ao natural (20×20)");
    assert_eq!(r("#alvo"), (1.0, 1.0, 100.0, 100.0), "100×100: a altura definite de #dentro transferida à largura pela razão 1:1, lida do SVG sem rasterizar");
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
/// `src` que agora RESOLVE (`tests/css/support/20x50-green.png`, commitado
/// no retrabalho deste lote — a primeira medição tinha saído com o Edge a
/// desenhar o ícone de imagem PARTIDA, 90×16, porque o PNG nunca tinha sido
/// commitado; re-medido com o ficheiro presente, o Blink deriva a altura
/// pela razão natural do PNG: 90 × (50/20) = 225).
///
/// O PNG real é de 1 BIT por píxel (paleta de uma entrada, 84 bytes) — o
/// formato que fazia o motor continuar a dar altura 0 mesmo com o ficheiro
/// presente, porque `imagem/png.rs` só aceitava 8 bits por canal
/// (retrabalho: `png.rs` ganhou suporte a 1/2/4 bits nos tipos 0/3, os
/// únicos que a spec permite nessas profundidades).
#[test]
fn largura_so_declarada_deriva_a_altura_pela_razao_do_png_de_1_bit() {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/css/support/20x50-green.png");
    let (rgba, w, h) = crate::imagem::png::decodificar(&std::fs::read(p).expect("png no repo")).expect("png de 1 bit");
    assert_eq!((w, h), (20, 50), "a fixture depende de um PNG 20×50 real");
    let html = r#"<style>#img1{width:90px;display:block}</style>
<img id="img1" src="support/20x50-green.png">"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#img1").unwrap();
        d.set_pixel_data(id, rgba, w, h);
    });
    let r = rect(&dom, &list, "#img1", 0);
    assert_eq!((r.w, r.h), (90.0, 225.0), "90 declarado; altura = 90 × (50/20), a razão natural do PNG");
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
