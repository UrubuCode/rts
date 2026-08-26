//! A auditoria de `calculos/estilo.jsonl`, EXECUTADA — lote B: a cascata.
//!
//! Onze registos que precisam de um DOM e de uma folha. **Oito confirmaram-se,
//! DOIS estavam obsoletos** — e os dois por causa de correções minhas
//! posteriores ao registo — e **um não se responde por este instrumento**.
//!
//! O caso dos obsoletos é o que vale a pena reter: um registo não envelhece só
//! por estar errado à nascença. Envelhece quando alguém corrige o código e não
//! volta ao mapa — e o mapa continua a dirigir trabalho com autoridade.

use crate::dom::parse_html_to_dom;

fn doc(estilo: &str, corpo: &str) -> crate::Dom {
    parse_html_to_dom(&format!(
        "<html><head><style>{estilo}</style></head><body>{corpo}</body></html>"
    ))
}
fn prop(d: &crate::Dom, sel: &str, p: &str) -> String {
    d.computed_property(d.query(sel).expect("selector sem nó"), p)
}

/// `estilo.atrule.import` — o `@import` é saltado e nenhuma regra da folha
/// importada existe. A regra a seguir a ele continua a ser lida, que é o que
/// distingue "saltado" de "o parse partiu-se ali".
#[test]
fn import_e_saltado_sem_partir_o_resto() {
    let d = doc("@import url(x.css); p{color:red}", "<p>x</p>");
    assert_eq!(prop(&d, "p", "color"), "rgb(255, 0, 0)");
}

/// `estilo.atrule.media-so-largura` — só `min-width`/`max-width` são avaliadas;
/// **qualquer outra feature DESLIGA o bloco inteiro**, em vez de o ignorar.
#[test]
fn so_min_e_max_width_sao_avaliadas() {
    let aplica = |q: &str| {
        let d = doc(&format!("@media {q} {{ p{{font-size:33px}} }}"), "<p>x</p>");
        prop(&d, "p", "font-size") == "33px"
    };
    assert!(aplica("(min-width:1px)"));
    assert!(aplica("(max-width:99999px)"));
    assert!(aplica("screen and (min-width:1px)"));
    // As que caem, e todas caem por serem desconhecidas e não por não casarem.
    assert!(!aplica("(min-height:1px)"), "min-height");
    assert!(!aplica("(orientation:landscape)"), "orientation");
    assert!(!aplica("(prefers-color-scheme: dark)"), "prefers-color-scheme");
    assert!(!aplica("(400px <= width < 99999px)"), "a sintaxe de intervalo");
}

/// `estilo.var.important` — custom properties mantêm a importância durante a
/// cascade por elemento, antes da substituição em `var()`.
#[test]
fn a_importancia_de_uma_custom_property_e_respeitada() {
    let d = doc(":root{--c:red !important;--c:blue} p{color:var(--c)}", "<p>x</p>");
    assert_eq!(
        prop(&d, "p", "color"),
        "rgb(255, 0, 0)",
        "a definição important deve vencer a normal"
    );
}

/// `estilo.cascata.presentational-hints` — nenhum atributo de apresentação entra
/// na cascata.
#[test]
fn os_atributos_de_apresentacao_nao_entram() {
    let d = parse_html_to_dom(
        "<html><body><table bgcolor=\"red\"><tr><td width=\"100\">x</td></tr></table></body></html>",
    );
    assert_eq!(prop(&d, "table", "background-color"), "rgba(0, 0, 0, 0)");
    assert_eq!(prop(&d, "td", "width"), "auto");
}

/// `estilo.seletor.nth-child-of-e-has` — `:has()` e a forma `of` do `nth-child`
/// não são reconhecidos, e a regra inteira é descartada.
#[test]
fn has_e_o_of_do_nth_child_derrubam_a_regra() {
    let d = doc("p:has(span){color:red}", "<p><span>x</span></p>");
    assert_eq!(prop(&d, "p", "color"), "rgb(0, 0, 0)");
    let d = doc("p:nth-child(1 of .x){color:red}", "<p class=x>y</p>");
    assert_eq!(prop(&d, "p", "color"), "rgb(0, 0, 0)");
}

/// `estilo.seletor.atributo-case-insensitive` — a flag `i` não existe.
#[test]
fn a_flag_i_de_atributo_nao_existe() {
    let d = doc("[data-a=\"V\" i]{color:red}", "<p data-a=\"v\">x</p>");
    assert_eq!(prop(&d, "p", "color"), "rgb(0, 0, 0)");
}

/// `estilo.enum.text-align-start-end` — `start`/`end` são congelados em
/// esquerda/direita NO PARSE, antes de a direção existir.
#[test]
fn start_e_end_sao_congelados_no_parse() {
    let d = doc("p{text-align:start}", "<p>x</p>");
    assert_eq!(prop(&d, "p", "text-align"), "left");
    let d = doc("p{text-align:end}", "<p>x</p>");
    assert_eq!(prop(&d, "p", "text-align"), "right");
}

/// `estilo.computado.blockificacao` — o `display` computado de um item de flex
/// NÃO é blockificado, e o `getComputedStyle` responde o que o autor escreveu.
#[test]
fn o_display_de_um_item_de_flex_nao_blockifica() {
    let d = doc(".f{display:flex}", "<div class=f><span id=s>x</span></div>");
    assert_eq!(prop(&d, "#s", "display"), "inline", "o Chrome responde block");
}

// ── Os dois registos que a execução encontrou OBSOLETOS ─────────────────────

/// `estilo.atrule.supports-nao-avaliado` — **obsoleto**: o registo diz que o
/// `@supports` é transparente e que aplicamos sempre o bloco. Já não é: passou a
/// ser avaliado. Este teste fixa o estado novo.
#[test]
fn supports_e_avaliado_o_registo_esta_obsoleto() {
    let d = doc(
        "@supports (xpto:1px){p{font-size:11px}} @supports not (xpto:1px){p{font-size:22px}}",
        "<p>x</p>",
    );
    assert_eq!(prop(&d, "p", "font-size"), "22px", "só um ramo do par entra");
}

/// `estilo.var.invalido-vira-unset` — **obsoleto, e por uma correção minha
/// posterior.**
///
/// O registo dizia que a declaração inválida APAGA a anterior. Deixou de apagar
/// quando o lote do `set_if` passou os ~176 sítios do dispatch a só escreverem
/// quando o parse dá resultado. Hoje o `red` sobrevive.
///
/// Continua a divergir do Chrome, e a nota do registo tem de o dizer com a
/// terceira forma: lá a declaração é *invalid at computed-value time* e cai em
/// `unset` (herda); aqui é descartada e a anterior fica. **Nem apaga nem herda.**
#[test]
fn uma_var_invalida_ja_nao_apaga_a_anterior() {
    let d = doc("p{color:red;color:var(--nao-existe)}", "<p>x</p>");
    assert_eq!(prop(&d, "p", "color"), "rgb(255, 0, 0)");
}


#[test]
fn unset_no_root_e_no_descendente_resolve_com_o_pai_correcto() {
    let d = doc(
        "html{color:red;color:unset} body{color:blue;color:unset}",
        "<p id=p>x</p>",
    );
    assert_eq!(prop(&d, ":root", "color"), "rgb(0, 0, 0)");
    assert_eq!(prop(&d, "body", "color"), "rgb(0, 0, 0)");
    assert_eq!(prop(&d, "#p", "color"), "rgb(0, 0, 0)");
}


#[test]
fn all_initial_no_autor_reseta_o_valor_ua() {
    let d = doc("p{color:red} #alvo{all:initial}", "<p id=alvo>x</p>");
    assert_eq!(prop(&d, "#alvo", "color"), "rgb(0, 0, 0)");
}
