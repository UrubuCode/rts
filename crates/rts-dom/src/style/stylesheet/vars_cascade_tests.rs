//! Herança e resolução de `var()` na cascade — as behaviours que a página real
//! (`google.html`) exercita e que a suíte antiga não cobria.
//!
//! Vive num ficheiro próprio, incluído por `#[path]` a partir de `stylesheet.rs`,
//! porque `stylesheet.rs` já está no seu teto de linhas e um `mod` novo em
//! `style/mod.rs` mexeria num ficheiro que outra pessoa está a editar.

use crate::dom::parse_html_to_dom;

/// `font-size` computado de um seletor, em texto de browser (`"14px"`).
fn font_size(dom: &crate::Dom, sel: &str) -> String {
    let n = dom.query(sel).unwrap();
    dom.computed_property(n, "font-size")
}

#[test]
fn variavel_do_root_alcanca_descendente_distante() {
    // A custom property é uma propriedade HERDADA: declarada no `:root`, vale em
    // todo o documento — não só nos elementos que casam a regra que a declarou.
    let dom = parse_html_to_dom(
        "<html><head><style>:root{--a:14px} body{font-size:var(--a)}</style></head>\
         <body><div id=\"d\"><span id=\"s\">x</span></div></body></html>",
    );
    assert_eq!(font_size(&dom, "body"), "14px", "o body vê a var do :root");
    assert_eq!(font_size(&dom, "#d"), "14px", "e o filho herda o VALOR");
    assert_eq!(font_size(&dom, "#s"), "14px", "e o neto também");
}

#[test]
fn nome_de_custom_property_distingue_maiusculas() {
    // Nomes de custom property são CASE-SENSITIVE (CSS Variables §2: "custom
    // property names are case-sensitive"), ao contrário dos nomes de propriedade
    // normais. `--Mhs7de` e `--mhs7de` são duas variáveis diferentes — e a página
    // do Google usa 80 nomes com maiúsculas em 91.
    let dom = parse_html_to_dom(
        "<html><head><style>:root{--Mhs7de:14px} body{font-size:var(--Mhs7de)}</style></head>\
         <body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(font_size(&dom, "body"), "14px", "o nome chega inteiro à cascade");
    assert_eq!(font_size(&dom, "#d"), "14px");
}

#[test]
fn nomes_que_so_diferem_no_caso_sao_variaveis_distintas() {
    // O reverso do teste acima: normalizar o caso não pode fundir dois nomes.
    let dom = parse_html_to_dom(
        "<html><head><style>:root{--A:10px;--a:30px} body{font-size:var(--A)}\
         #d{font-size:var(--a)}</style></head><body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(font_size(&dom, "body"), "10px");
    assert_eq!(font_size(&dom, "#d"), "30px");
}

#[test]
fn redefinicao_num_nivel_intermedio_so_afeta_a_subarvore() {
    // Herança é por ELEMENTO e o valor herdado é o do PAI já resolvido: um filho
    // que redefine `--a` afeta a si e aos descendentes, nunca os irmãos.
    let dom = parse_html_to_dom(
        "<html><head><style>:root{--a:14px} div{font-size:var(--a)} #meio{--a:20px}\
         </style></head><body><div id=\"meio\"><div id=\"dentro\">x</div></div>\
         <div id=\"fora\">y</div></body></html>",
    );
    assert_eq!(font_size(&dom, "#meio"), "20px", "a redefinição vale nele");
    assert_eq!(font_size(&dom, "#dentro"), "20px", "e desce");
    assert_eq!(font_size(&dom, "#fora"), "14px", "o irmão fica com o :root");
}

#[test]
fn var_que_nao_resolve_cai_para_o_herdado() {
    // Sem valor nem fallback a declaração é "inválida no tempo de computação"
    // (spec) — cai para o herdado, não para lixo. Aqui o pai declara 22px.
    let dom = parse_html_to_dom(
        "<html><head><style>body{font-size:22px} #d{font-size:var(--nao-existe)}\
         </style></head><body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(font_size(&dom, "#d"), "22px");
}

#[test]
fn var_que_nao_resolve_usa_o_fallback() {
    let dom = parse_html_to_dom(
        "<html><head><style>body{font-size:22px} #d{font-size:var(--nao-existe, 14px)}\
         </style></head><body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(font_size(&dom, "#d"), "14px");
}




#[test]
fn root_casa_o_html_mesmo_com_uma_folha_antes_dele() {
    // Um `<style>` antes do `<html>` fica IRMÃO dele (o parser recusa-lhe
    // estrutura implícita), portanto o documento tem dois elementos de topo. O
    // `:root` continua a ser o `<html>` — é o que o browser faz, e é o que faz a
    // folha do Google chegar ao documento: ela declara as variáveis todas em
    // `:root` e o harness de paridade cola a folha à frente da página.
    let dom = parse_html_to_dom(
        "<style>:root{--a:14px} body{font-size:var(--a)}</style>\
         <html><body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(dom.query_all(":root").len(), 1, ":root casa o <html>, não zero");
    assert_eq!(font_size(&dom, "body"), "14px");
    assert_eq!(font_size(&dom, "#d"), "14px");
}

#[test]
fn folha_antes_do_html_aplica_as_regras_que_nao_dependem_do_root() {
    // Pin do que NUNCA esteve partido: uma folha antes do `<html>` sempre foi
    // parseada e as suas regras sempre casaram. Só o `:root` falhava — e foi essa
    // distinção que evitou reescrever o parser de HTML para corrigir um seletor.
    let dom = parse_html_to_dom(
        "<style>body{font-size:14px} p{color:#ff0000}</style>\
         <html><body><p id=\"p\">x</p></body></html>",
    );
    let p = dom.query("#p").unwrap();
    assert_eq!(font_size(&dom, "body"), "14px");
    assert_eq!(dom.computed_property(p, "color"), "rgb(255, 0, 0)");
}

#[test]
fn a_pagina_do_harness_de_paridade_resolve_as_variaveis() {
    // A forma exata que `scripts/parity/run.sh` monta: `<style>` + CSS +
    // `</style>` + o documento, doctype incluído. O Chrome move a folha para o
    // `head` implícito e as variáveis valem; aqui a folha fica de fora e é o
    // `:root` que tem de continuar a encontrar o `<html>`.
    let dom = parse_html_to_dom(
        "<style>:root{--a:14px} body,input,button{font-size:var(--a)}</style>\n\
         <!doctype html><html><head></head><body><div id=\"d\">x</div></body></html>",
    );
    assert_eq!(font_size(&dom, "body"), "14px");
    assert_eq!(font_size(&dom, "#d"), "14px", "e desce por herança");
}

#[test]
fn fragmento_sem_html_continua_a_ter_um_unico_root() {
    // O caso para que a contagem `== 1` foi escrita, e que o fix não pode perder:
    // sem `<html>`, dois elementos de topo não têm raiz (o browser tem sempre uma
    // só), e um tem.
    let um = parse_html_to_dom("<div id=\"a\">x</div>");
    assert_eq!(um.query_all(":root").len(), 1, "o <html> inventado é a raiz");
    let frag = crate::dom::parse_fragmento("<div>a</div><div>b</div>");
    assert_eq!(frag.query_all(":root").len(), 0, "dois topos, nenhuma raiz");
}



#[test]
fn custom_property_important_vence_antes_do_var() {
    let d = parse_html_to_dom(
        "<html><head><style>p{--size:10px} p{--size:20px!important} \
         p{font-size:var(--size)}</style></head><body><p id=p>x</p></body></html>",
    );
    assert_eq!(font_size(&d, "#p"), "20px");
}

#[test]
fn custom_property_important_inline_vence_a_folha() {
    let d = parse_html_to_dom(
        "<html><head><style>p{--size:10px!important;font-size:var(--size)}</style></head>\
         <body><p id=p style=\"--size:30px!important\">x</p></body></html>",
    );
    assert_eq!(font_size(&d, "#p"), "30px");
}
