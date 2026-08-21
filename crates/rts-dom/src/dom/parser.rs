//! O parser de HTML: tags void, fechamento implícito, atributos, e a
//! construção da árvore a partir da string.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// Tags VAZIAS (void) da spec HTML — não têm fechamento nem filhos, logo NUNCA
/// empilham como "elemento aberto". Lista COMPLETA do HTML5 (whatwg
/// §void-elements): antes faltavam `area/base/col/embed/source/track/wbr`, e um
/// `<source>` dentro de `<video>` empilhava sem nunca fechar — o resto do
/// documento inteiro virava descendente dele.
pub(in crate::dom) fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Elementos de BLOCO cuja tag de ABERTURA fecha implicitamente um `<p>` aberto
/// (HTML5, tag omission do `p`: developer.mozilla.org/docs/Web/HTML/Element/p).
/// É a regra de fim-omitido que MAIS aparece em páginas reais — `<p>texto<div>`
/// põe o `div` como IRMÃO do `p`, nunca filho. Tabela como DADOS (uma lista num
/// único lugar), não um emaranhado de `if`s.
fn closes_open_p(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "details"
            | "div"
            | "dl"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}

/// `true` se a ABERTURA de `new_tag` fecha implicitamente `open_tag` quando este
/// é o TOPO da pilha de abertos (subconjunto das regras de tag-omission do HTML5
/// que mais dói em páginas reais: `<li>` sem `</li>`, `<p>` sem `</p>`, células
/// de tabela). IMPORTANTE: o chamador só aplica isto ao TOPO da pilha, em loop —
/// nunca fechamos "através" de um container (um `<li>` novo NÃO fecha o `<li>`
/// de um `<ul>` ancestral: se o topo é `ul`, nada casa e nada fecha).
/// As tags que o `<head>` aceita. Qualquer outra ABRE o `<body>` — é a regra
/// de omissão de tags do HTML, e não uma tolerância a HTML malformado: uma
/// página real pode não escrever `<body>` nenhum e o browser insere um.
///
/// Sem isto o `web.whatsapp.com` — que omite as três — punha os `<div>` do app
/// dentro do `<head>`, e cada regra `body { … }` da folha dele (a que dá
/// `height: 100%` e a cor do texto) não casava com elemento nenhum.
fn allowed_in_head(tag: &str) -> bool {
    matches!(
        tag,
        "base"
            | "basefont"
            | "bgsound"
            | "link"
            | "meta"
            | "noscript"
            | "script"
            | "style"
            | "template"
            | "title"
    )
}

fn implicitly_closes(new_tag: &str, open_tag: &str) -> bool {
    let same_kind = match new_tag {
        // um <li> novo termina o <li> corrente (viram irmãos, não aninhados).
        "li" => open_tag == "li",
        // <dt>/<dd> terminam o <dt>/<dd> corrente (termo/definição irmãos).
        "dt" | "dd" => matches!(open_tag, "dt" | "dd"),
        // <option> termina o <option> corrente.
        "option" => open_tag == "option",
        // um <tr> novo termina a célula aberta E o <tr> corrente: o loop do
        // chamador fecha o td/th que estiver no topo e depois o tr exposto.
        "tr" => matches!(open_tag, "td" | "th" | "tr"),
        // uma célula nova termina a célula corrente (mas NÃO o tr — a nova
        // célula nasce dentro da mesma linha).
        "td" | "th" => matches!(open_tag, "td" | "th"),
        _ => false,
    };
    // Regra dos blocos: a abertura de qualquer elemento de bloco fecha um <p>
    // aberto (inclui `<p>` novo fechando `<p>` corrente — p está na tabela).
    same_kind || (open_tag == "p" && closes_open_p(new_tag))
}

// A herança de CSS (`inherit_from`) e o gatilho de transição (`differs_animated`)
// são GERADOS pela tabela de propriedades `css_props!` em `style/props.rs` — a
// lista de campos herdáveis/animáveis vive SÓ lá (as versões locais campo-a-campo
// que moravam aqui dessincronizavam da interpolação do anim.rs).


/// Parseia a parte crua de atributos de uma tag (`class='card' id="x" checked`)
/// em pares `Attr`. Tolerante: aceita aspas simples/duplas ou sem aspas, e
/// atributo sem valor (`checked` → value vazio). Nomes em minúsculas; valores
/// com entidades decodificadas. Não é conforme à spec — cobre o uso comum.
fn parse_attrs(raw: &str) -> Vec<Attr> {
    let mut attrs = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Pula espaços entre atributos.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Lê o nome até `=`, espaço ou fim.
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if i == name_start {
            break; // nada de nome — acabou.
        }
        let name = raw[name_start..i].to_ascii_lowercase();
        // Pula espaços antes de um possível `=`.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && bytes[i] == b'=' {
            i += 1; // consome `=`
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                // Valor entre aspas: lê até a aspa de fechamento igual.
                let quote = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let v = raw[v_start..i].to_string();
                if i < bytes.len() {
                    i += 1; // consome a aspa de fechamento
                }
                v
            } else {
                // Valor sem aspas: lê até o próximo espaço.
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                raw[v_start..i].to_string()
            }
        } else {
            String::new() // atributo booleano (sem `=valor`).
        };
        // No HTML, um atributo duplicado é ignorado depois da primeira ocorrência.
        if !attrs.iter().any(|a: &Attr| a.name == name) {
            crate::bump!(attrs_parsed);
            attrs.push(Attr {
                name,
                value: crate::html::decode_entities(&value),
            });
        } else {
            crate::bump!(attrs_duplicated);
        }
    }
    attrs
}


/// Parseia HTML para uma árvore retida. Reusa o tokenizador de `html.rs`; a
/// diferença é a etapa sintática: aqui mantém-se uma PILHA de "elemento aberto"
/// e cada nó nasce filho do topo da pilha.
///
/// - Tag de abertura → primeiro aplica o AUTO-FECHAMENTO IMPLÍCITO do HTML5
///   (`implicitly_closes`: `<li>` fecha `<li>`, bloco fecha `<p>`, `<tr>` fecha
///   `td/th`+`tr`…), depois cria `Element` filho do topo e empurra na pilha
///   (salvo void, que não empurra).
/// - Tag de fechamento → faz pop até casar o nome (tolerante a aninhamento
///   malformado; um `</x>` sem `<x>` aberto é ignorado).
/// - Texto → vira nó `Text` filho do topo (whitespace puro entre tags é
///   descartado, como no caminho immediate-mode, para a árvore não encher de
///   nós de espaço irrelevantes).
pub fn parse_html_to_dom(html: &str) -> Dom {
    parse_com_estrutura(html, true)
}

/// O mesmo parser, sem inventar `<html>`/`<body>`.
///
/// É o que o `innerHTML` precisa: o conteúdo entra DENTRO de um elemento que já
/// existe, e a estrutura do documento já foi decidida quando a página foi
/// parseada. Sem esta distinção, `el.innerHTML = "<p>x</p>"` punha um
/// `<html><body>` inteiro dentro do `el` — o que o browser nunca faz, e o que
/// nenhum programa que leia a árvore a seguir espera.
pub fn parse_fragmento(html: &str) -> Dom {
    parse_com_estrutura(html, false)
}

fn parse_com_estrutura(html: &str, estrutura: bool) -> Dom {
    // Instala a UA-stylesheet (defaults de display/margem das tags HTML) na primeira
    // vez — em Rust, como DADOS (tabela em block.rs), rodando só quando há DOM. NÃO é
    // mais um prelude `.ts` (isso quebrava todo programa: o `ua.ts` chamava `dom.*`
    // no top-level e `dom` é unbound sem `import "rts:dom"`). Idempotente.
    let _phase = crate::metrics::phases::scope("load-html");
    crate::block::install_ua_defaults();
    crate::bump!(html_bytes, html.len());
    let mut dom = Dom::new();
    // Pilha de (índice cru aberto, nome da tag). Começa na raiz Document.
    let mut open: Vec<(NodeIdx, String)> = vec![(dom.root, String::new())];

    for tok in tokenize(html) {
        match tok {
            Token::Tag {
                name,
                attrs_raw,
                close,
            } => {
                if close {
                    // Pop até encontrar a tag de nome igual (tolerante).
                    if let Some(pos) = open.iter().rposition(|(_, n)| *n == name) {
                        // Fecha esse nível e quaisquer filhos mal-fechados acima.
                        crate::bump!(tags_unclosed_at_eof, open.len().saturating_sub(pos + 1));
                        open.truncate(pos);
                    } else {
                        crate::bump!(tags_orphan_close);
                        crate::note!("tag-de-fechamento-orfa", format!("</{name}>"));
                    }
                    // `</x>` órfão (sem abertura): ignora, não mexe na pilha.
                } else {
                    // AUTO-FECHAMENTO IMPLÍCITO (HTML5 tag omission, subconjunto):
                    // antes de abrir, fecha a(s) tag(s) do TOPO da pilha que a
                    // nova tag termina. Em loop porque `<tr>` pode ter que fechar
                    // a célula (`td`/`th`) E o `tr` empilhados; só o topo é
                    // inspecionado a cada passo — nunca um ancestral através de
                    // um container (ver `implicitly_closes`). O guard `> 1`
                    // preserva a raiz `#document`.
                    while open.len() > 1 && implicitly_closes(&name, &open.last().unwrap().1) {
                        crate::bump!(tags_implicitly_closed);
                        open.pop();
                    }
                    if estrutura {
                        open_implicit_body(&mut dom, &mut open, &name);
                    }
                    let parent = open.last().unwrap().0;
                    let attrs = parse_attrs(&attrs_raw);
                    let id = dom.push(NodeKind::Element { tag: name.clone() }, attrs, parent);
                    if is_void(&name) {
                        crate::bump!(tags_void);
                    } else {
                        open.push((id, name));
                    }
                }
            }
            Token::Text(text) => {
                // O DOM do browser preserva whitespace entre elementos como nós de
                // texto. O layout decide depois se esse whitespace colapsa visualmente.
                if !text.is_empty() {
                    let parent = open.last().unwrap().0;
                    dom.push(NodeKind::Text(text), Vec::new(), parent);
                }
            }
            Token::Comment(content) => {
                // DOM fiel preserva comentários como nós (nodeType 8); o render os
                // ignora. Conteúdo cru (sem decodificar entidades).
                let parent = open.last().unwrap().0;
                dom.push(NodeKind::Comment(content), Vec::new(), parent);
            }
            Token::RawElement {
                tag,
                attrs,
                content,
            } => {
                // `<style>`/`<script>`: DOM fiel preserva o ELEMENTO (com o texto cru
                // como filho), mas o conteúdo NÃO é HTML. Para `<style>`, o CSS
                // alimenta o stylesheet de autor (a cascade de `computed_style`).
                // Para `<script>`, só preserva o nó (não executamos JS). O render
                // ignora ambos (sem `BlockDef`/inline para essas tags). Os atributos
                // da abertura são preservados (`<script src>`/`<style media>`).
                if tag == "style" {
                    dom.add_stylesheet(&content);
                }
                let parent = open.last().unwrap().0;
                let parsed = parse_attrs(&attrs);
                let el = dom.push(NodeKind::Element { tag }, parsed, parent);
                if !content.is_empty() {
                    dom.push(NodeKind::Text(content), Vec::new(), el);
                }
            }
        }
    }
    dom
}

/// Fecha o `<head>` e abre um `<body>` quando a tag que vem a seguir não pode
/// viver no head — a inserção implícita que a spec do HTML manda fazer.
///
/// Só actua dentro de um `<head>` ainda aberto: uma página que escreve as três
/// tags passa por aqui sem efeito, e uma que não escreve nenhuma ganha o
/// `<body>` no primeiro elemento de fluxo. O que NÃO faz é inserir `<html>` ou
/// `<head>` ausentes — a árvore continua a aceitar um documento sem eles, e
/// nenhuma regra CSS depende desses dois da forma que depende do `body`.
fn open_implicit_body(dom: &mut Dom, open: &mut Vec<(NodeIdx, String)>, new_tag: &str) {
    // As três tags da estrutura nunca abrem uma estrutura implícita: são elas.
    // Sem `html` nesta lista, um documento que traga o que quer que seja antes
    // do `<html>` — um `<style>` injetado, um comentário — fazia nascer um
    // `<html>` implícito e o `<html>` REAL ficava dentro dele. A árvore ainda
    // parecia razoável num `dump`, mas todo o caminho de elemento
    // (`html[1]/body[1]/…`) ganhava um nível, e uma comparação contra o browser
    // deixava de encontrar os mesmos nós: de 16 813 caminhos comuns passaram a
    // ser 2.
    if matches!(new_tag, "html" | "head" | "body") || allowed_in_head(new_tag) {
        return;
    }
    // Já estamos DENTRO de um `<body>`? Então não há nada a abrir.
    if open.iter().any(|(_, n)| n == "body") {
        return;
    }
    // Um `<head>` aberto fecha-se aqui: a primeira tag de fluxo termina-o.
    if let Some(pos) = open.iter().rposition(|(_, n)| n == "head") {
        open.truncate(pos);
    }
    // E o `<html>`, se também não existir. Sem isto um fragmento sem NENHUMA das
    // três tags — `<style>body{…}</style><p>x</p>`, que é o que qualquer teste
    // escreve e o que um `innerHTML` recebe — deixava o `<p>` solto no
    // `#document`, e as regras `html{…}`/`body{…}` não casavam com elemento
    // nenhum. Toda a propriedade HERDADA declarada aí (a cor, a fonte, o
    // `line-height`) desaparecia em silêncio: a herança funcionava, o ancestral
    // é que não existia.
    if !open.iter().any(|(_, n)| n == "html") {
        let raiz = open.last().unwrap().0;
        let html = dom.push(
            NodeKind::Element {
                tag: "html".to_owned(),
            },
            Vec::new(),
            raiz,
        );
        open.push((html, "html".to_owned()));
    }
    let parent = open.last().unwrap().0;
    let body = dom.push(
        NodeKind::Element {
            tag: "body".to_owned(),
        },
        Vec::new(),
        parent,
    );
    open.push((body, "body".to_owned()));
}
