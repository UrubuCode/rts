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

