//! O INVARIANTE DA UNIÃO: a caixa de um inline contém a dos seus fragmentos.
//!
//! Não é uma preferência deste motor — é a definição. A caixa de um elemento
//! inline é o bounding box do que ele gera, que é o que a spec manda o
//! `getBoundingClientRect` devolver; um pai sem caixa nenhuma cujo filho ESTÁ
//! posicionado é impossível, independentemente da causa.
//!
//! Existe porque aconteceu. Na Wikipédia, 397 `<span rel="mw:referencedBy">`
//! — os retrolinks da lista de referências — ficaram com `0x0` enquanto o `<a>`
//! lá dentro continuava a medir `7x17`. O Chrome dá 11x16 ao pai e 11x16 ao
//! filho: exatamente a união. O defeito não era novo; ficou visível quando
//! `display:none` passou a significar mesmo "não gera caixa" e deixou de haver
//! uma caixa errada a tapá-lo.
//!
//! Módulo próprio e não `flowtests.rs` porque aquele diz, no seu cabeçalho, que
//! é o fluxo de BLOCO. E a asserção aqui é sobre uma propriedade da árvore, não
//! sobre um valor esperado: um teste que fixasse `11x16` estaria a fixar o
//! medidor aproximado junto com o invariante, e falharia por calibração no dia
//! em que o avanço por carácter mudasse — que é uma coisa que já mudou duas
//! vezes.

use crate::layout::Rect;
use crate::table::tests::geometria;

/// O rect de um elemento, ou `None` se ele não recebeu geometria nenhuma.
///
/// O `rect` de `table::tests` entra em pânico nesse caso, e aqui a AUSÊNCIA é
/// precisamente um dos desfechos que se quer distinguir de `0x0`: são dois
/// estados diferentes do motor com o mesmo sintoma no ecrã.
fn caixa(
    dom: &crate::Dom,
    list: &crate::layout::DisplayList,
    sel: &str,
    n: usize,
) -> Option<Rect> {
    let ids = dom.query_all(sel);
    let id = *ids.get(n)?;
    let idx = dom.resolve(id)?;
    list.geometry_now().rects.get(&idx).copied()
}

/// Um rect com área. `0x0` não é caixa: é o que o motor responde quando não
/// mediu nada, e foi a resposta que estes 397 elementos passaram a dar.
fn tem_area(r: Rect) -> bool {
    r.w > 0.0 && r.h > 0.0
}

/// A asserção central, escrita uma vez: se o FILHO tem caixa, o PAI tem de ter
/// uma que o contenha.
fn pai_contem_filho(html: &str, sel_pai: &str, sel_filho: &str) {
    let (dom, list) = geometria(html, 600.0);
    let filho = caixa(&dom, &list, sel_filho, 0)
        .unwrap_or_else(|| panic!("{sel_filho} sem geometria — o caso não testa nada"));
    assert!(
        tem_area(filho),
        "{sel_filho} mediu {}x{} — o caso não testa nada sem um filho posicionado",
        filho.w,
        filho.h
    );

    let pai = caixa(&dom, &list, sel_pai, 0);
    let Some(pai) = pai else {
        panic!(
            "{sel_pai} não recebeu geometria NENHUMA enquanto {sel_filho} mediu {}x{} — \
             a união de um inline é o bounding box dos seus fragmentos",
            filho.w, filho.h
        );
    };
    assert!(
        tem_area(pai),
        "{sel_pai} mediu {}x{} enquanto {sel_filho} mediu {}x{} — \
         um pai sem caixa com um filho posicionado é impossível pela definição da união",
        pai.w,
        pai.h,
        filho.w,
        filho.h
    );
    // E não basta ter área: tem de CONTER. Um pai que medisse o seu próprio
    // texto e ignorasse o filho passaria na asserção de cima.
    assert!(
        pai.x <= filho.x + 0.5
            && pai.y <= filho.y + 0.5
            && pai.x + pai.w + 0.5 >= filho.x + filho.w
            && pai.y + pai.h + 0.5 >= filho.y + filho.h,
        "{sel_pai} ({},{} {}x{}) não contém {sel_filho} ({},{} {}x{})",
        pai.x,
        pai.y,
        pai.w,
        pai.h,
        filho.x,
        filho.y,
        filho.w,
        filho.h
    );
}

/// O caso simples, e é o que fixa a regra: um inline cujo único conteúdo é
/// outro inline mede o que esse outro mede.
#[test]
fn um_inline_cujo_unico_filho_e_inline_contem_a_caixa_do_filho() {
    pai_contem_filho("<div><span><a>xy</a></span></div>", "span", "a");
}

/// A FORMA DA WIKIPÉDIA, e a razão de este ficheiro existir: o `<a>` não tem
/// texto próprio — todo o conteúdo visível dele é gerado — e o único filho de
/// elemento que tem está `display:none`.
///
/// O que se afirma aqui é só a metade que não depende de conteúdo gerado: se o
/// `<a>` receber uma caixa, o `<span>` que o envolve recebe uma que a contenha.
/// A outra metade — o `::before` com `counter()` que dá ao `<a>` os seus 10x16
/// no Chrome — é uma lacuna nomeada em `docs/ui/estado-motor-css.md` e não se
/// finge que está resolvida aqui.
#[test]
fn o_retrolink_de_referencia_nao_perde_o_pai_quando_o_conteudo_esta_escondido() {
    pai_contem_filho(
        "<div><span><a>1<span style='display:none'>oculto</span></a></span></div>",
        "span",
        "a",
    );
}

/// Dois fragmentos e um pai: a união é sobre TODOS, não sobre o primeiro. Este
/// é o caso que um `union_rect` que devolvesse a caixa do primeiro filho
/// passaria por acidente nos dois testes de cima.
#[test]
fn um_inline_com_dois_filhos_inline_contem_o_segundo_tambem() {
    let html = "<div><span><a>xy</a><b>zw</b></span></div>";
    pai_contem_filho(html, "span", "a");
    pai_contem_filho(html, "span", "b");
}

/// A REPRODUÇÃO, reduzida ao mínimo: o filho recebe a sua caixa de conteúdo
/// GERADO (`::before`) e o pai não recebe nenhuma.
///
/// Foi encontrada por eliminação, e o que a distingue dos três casos acima é a
/// PROVENIÊNCIA da caixa do filho, não a forma da árvore: com texto real no
/// `<a>` o `<span>` mede-se; com a mesma árvore e o texto a vir do `::before`,
/// o `<span>` não recebe entrada nenhuma em `rects`. Um fragmento gerado é um
/// fragmento — conta para a união do pai como qualquer outro.
///
/// É esta a forma dos 397 retrolinks: todo o conteúdo visível deles é gerado.
#[test]
fn um_inline_cujo_filho_so_tem_conteudo_gerado_ainda_contem_esse_filho() {
    pai_contem_filho(
        "<style>a::before{content:'x'}</style><div><span><a></a></span></div>",
        "span",
        "a",
    );
}

/// O CASO LEGÍTIMO que o invariante não pode partir: um inline cujos filhos
/// estão TODOS `display:none` fica mesmo sem fragmentos, e o pai com ele.
///
/// A diferença é ter ou não um filho COM caixa — não é o `display:none` em si.
/// Está aqui porque a correção da união mexe em quem recebe `union_rect`, e a
/// maneira mais fácil de a fazer render era dar caixa a quem não gera nenhuma:
/// isso passaria nos quatro testes acima e inventaria geometria na página toda.
#[test]
fn um_inline_com_todos_os_filhos_escondidos_continua_sem_caixa() {
    let (dom, list) = geometria(
        "<div><span><a><span style='display:none'>oculto</span></a></span></div>",
        600.0,
    );
    for (sel, n) in [("span", 0), ("a", 0)] {
        let r = caixa(&dom, &list, sel, n);
        assert!(
            r.is_none_or(|r| !tem_area(r)),
            "{sel} recebeu {r:?} sem ter fragmento nenhum para conter"
        );
    }
}
