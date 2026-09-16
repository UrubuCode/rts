//! CONTEÚDO GERADO — `::before` e `::after`.
//!
//! Um pseudo-elemento não é um nó do DOM: é uma caixa que a cascata manda
//! existir. Isso obriga a uma decisão de arquitetura logo à entrada, e a que
//! está tomada aqui é: **nada é acrescentado à árvore de nós**. Um `::before`
//! não aparece em `childNodes`, `childCount`, `querySelectorAll` nem no
//! `innerHTML`, exatamente como no browser — e a forma de garantir isso não é
//! filtrar em cada uma dessas consultas (que seria preciso lembrar em todas as
//! futuras), mas nunca criar o nó.
//!
//! A alternativa rejeitada foi a do Blink, que cria um `PseudoElement` real
//! ligado ao elemento originante e o mantém fora da lista de filhos. Faz
//! sentido lá, onde a árvore de layout é uma estrutura separada da árvore de
//! nós; aqui o layout é indexado por `NodeIdx` e um nó a mais no arena teria de
//! ser criado e destruído a cada recascata (a existência da caixa depende da
//! cascata), tocando no memo por epoch e na numeração documental. O custo
//! estava todo fora do problema.
//!
//! O que se faz em vez disso: a caixa gerada é resolvida sob procura, a partir
//! do elemento originante, e entregue ao fluxo inline como um RUN de texto — a
//! representação que o fluxo já tem para "texto com um estilo, pertencente a um
//! elemento". Ver [`crate::layout`], onde entra, e o teste
//! `before_nao_muda_a_arvore_de_nos`, que é o que prova que a árvore ficou
//! limpa.

use crate::style::ComputedStyle;

/// Uma caixa gerada, já resolvida: o texto que ela pinta e o estilo com que o
/// pinta.
#[derive(Clone, Debug, PartialEq)]
pub struct PseudoBox {
    /// O texto de `content`, já com `attr()` substituído.
    pub texto: String,
    /// O estilo computado da caixa — herdado do elemento originante e depois
    /// sobreposto pelas regras `::before`/`::after` que casaram.
    pub css: ComputedStyle,
}

/// O valor de `content`, decomposto nas peças que a spec permite concatenar
/// (`content: "[" attr(data-x) "]"`).
///
/// `content` não é uma propriedade do [`ComputedStyle`]: só se aplica a
/// pseudo-elementos, e pô-la na tabela de propriedades daria um campo a mais em
/// cada um dos milhares de `ComputedStyle` de uma página para servir umas
/// dezenas de caixas. Fica guardada na regra, ao lado das declarações.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// `none` / `normal` — não gera caixa nenhuma.
    Nenhum,
    /// As peças a concatenar, em ordem.
    Pecas(Vec<Peca>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Peca {
    /// Uma string literal do CSS, com os escapes já resolvidos.
    Texto(String),
    /// `attr(nome)` — o valor do atributo do elemento ORIGINANTE (não da caixa,
    /// que não tem atributos). Ausente resolve para string vazia, como na spec.
    Attr(String),
    /// `counter(nome)` / `counter(nome, estilo)` — o valor do contador de
    /// documento visível a esta caixa, escrito no sistema de numeração dado
    /// (`decimal` por omissão). Ver [`crate::counters`], que é quem o calcula.
    Contador(String, crate::style::ListStyleType),
    /// `open-quote` — o texto de abertura do par de `quotes` ativo no nível
    /// atual, que incrementa a seguir. Ver [`crate::quotes`].
    AbreAspas,
    /// `close-quote` — desce o nível e insere o texto de fecho do par que
    /// está a terminar.
    FechaAspas,
    /// `no-open-quote` — incrementa o nível sem inserir texto.
    NaoAbreAspas,
    /// `no-close-quote` — desce o nível sem inserir texto.
    NaoFechaAspas,
}

/// Parseia o valor de uma declaração `content`.
///
/// `None` significa "não sei gerar isto", e é diferente de
/// [`Content::Nenhum`]: o primeiro descarta a declaração e deixa a cascata
/// continuar com o que outra regra disser, o segundo é a resposta `none` da
/// própria folha e vence como qualquer outro valor.
///
/// FICAM DE FORA, e é aqui que se diz quais e porquê:
/// - `url(...)` — gera uma caixa SUBSTITUÍDA (uma imagem), que não é texto e
///   precisa do caminho de imagem do layout, com carregamento e tamanho
///   intrínseco. Medido na folha da Wikipédia: 6 das 100 regras.
/// - `counters(...)` — o PLURAL, que junta a pilha de escopos com um separador.
///   Zero ocorrências nas quatro folhas do corpus (`pagina.css`, `google.css`,
///   `wa.css`, `wa-app.css`), contra oito do singular. Recusado por nome e não
///   por acidente de parse, para não ser confundido com o singular e pintar um
///   número sem os antepassados.
/// - `var(...)` — o valor de uma custom property só se resolve POR ELEMENTO, e
///   o `content` é parseado uma vez ao ler a folha. Duas das oito ocorrências de
///   `counter()` da folha da Wikipédia estão nesta forma; ambas perdem a cascata
///   para uma regra posterior com o estilo literal, que é o que se pinta.
///
/// `open-quote`/`close-quote`/`no-open-quote`/`no-close-quote` JÁ NÃO ficam de
/// fora (causa dos 64 falhos WPT `CSS2/generated-content` triados em
/// 2026-09-16: a maior família era `quotes-*`/`quotes-applies-to-*`) — ver
/// [`Peca::AbreAspas`] e [`crate::quotes`], que resolve o par e a profundidade.
///
/// Em todos os que ficam de fora, gerar uma caixa vazia seria pior do que não
/// gerar: reservaria espaço e deslocaria o que está à volta sem pintar nada.
pub fn parse_content(valor: &str) -> Option<Content> {
    let v = valor.trim();
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("normal") {
        return Some(Content::Nenhum);
    }
    let mut pecas = Vec::new();
    let mut resto = v;
    while !resto.trim().is_empty() {
        resto = resto.trim_start();
        let primeiro = resto.chars().next()?;
        if primeiro == '"' || primeiro == '\'' {
            let (texto, depois) = string_css(resto)?;
            pecas.push(Peca::Texto(texto));
            resto = depois;
        } else if let Some(depois) = tira_prefixo_sem_caso(resto, "attr(") {
            let fecha = depois.find(')')?;
            let nome = depois[..fecha].trim().to_ascii_lowercase();
            if nome.is_empty() {
                return None;
            }
            pecas.push(Peca::Attr(nome));
            resto = &depois[fecha + 1..];
        } else if let Some(depois) = tira_prefixo_sem_caso(resto, "counter(") {
            let fecha = depois.find(')')?;
            let (nome, estilo) = counter_args(&depois[..fecha])?;
            pecas.push(Peca::Contador(nome, estilo));
            resto = &depois[fecha + 1..];
        } else if let Some((peca, resto2)) = palavra_de_aspas(resto) {
            pecas.push(peca);
            resto = resto2;
        } else {
            return None; // url(), counters(), um identificador solto…
        }
    }
    (!pecas.is_empty()).then_some(Content::Pecas(pecas))
}

/// Os argumentos de `counter(…)`: o nome e o sistema de numeração.
///
/// `None` recusa a declaração inteira, e é o que acontece com
/// `counter(x, var(--y))`: o primeiro `)` do texto fecha o `var`, o segundo
/// argumento chega partido e nenhum `ListStyleType` o reconhece. É o
/// comportamento que se quer — descartar a declaração deixa a cascata escolher
/// outra regra, enquanto adivinhar `decimal` pintaria um estilo que a folha não
/// pediu.
///
/// Um estilo que não conhecemos também recusa, em vez de cair em `decimal`: a
/// spec manda o *fallback*, mas aqui `decimal` seria um NÚMERO onde a folha
/// pediu letras — um erro com aparência de acerto, que é o que esta casa não
/// entrega.
fn counter_args(args: &str) -> Option<(String, crate::style::ListStyleType)> {
    let mut it = args.splitn(2, ',');
    let nome = it.next()?.trim();
    if nome.is_empty() || nome.contains(char::is_whitespace) {
        return None;
    }
    let estilo = match it.next() {
        None => crate::style::ListStyleType::Decimal,
        Some(s) => crate::style::ListStyleType::parse(&s.trim().to_ascii_lowercase())?,
    };
    Some((nome.to_string(), estilo))
}

/// `s` sem o prefixo `pref`, comparado sem distinguir maiúsculas.
fn tira_prefixo_sem_caso<'a>(s: &'a str, pref: &str) -> Option<&'a str> {
    (s.len() >= pref.len() && s[..pref.len()].eq_ignore_ascii_case(pref)).then(|| &s[pref.len()..])
}

/// Reconhece uma das quatro palavras-chave de aspas no início de `s` — o
/// nome tem de casar INTEIRO (terminar em espaço, fim de string ou `"`/`'`
/// seguinte), senão `open-quote-ish` seria lido como `open-quote`. Testa
/// `no-open-quote`/`no-close-quote` ANTES das formas curtas: as quatro são
/// palavras distintas e não prefixos umas das outras, mas testar as curtas
/// primeiro seria a armadilha simétrica de `counters(`/`counter(` que o
/// comentário do `parse_content` já nomeia.
fn palavra_de_aspas(s: &str) -> Option<(Peca, &str)> {
    const PALAVRAS: [(&str, fn() -> Peca); 4] = [
        ("no-open-quote", || Peca::NaoAbreAspas),
        ("no-close-quote", || Peca::NaoFechaAspas),
        ("open-quote", || Peca::AbreAspas),
        ("close-quote", || Peca::FechaAspas),
    ];
    for (nome, faz) in PALAVRAS {
        let Some(depois) = tira_prefixo_sem_caso(s, nome) else {
            continue;
        };
        let fim_de_palavra = depois
            .chars()
            .next()
            .map(|c| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(true);
        if fim_de_palavra {
            return Some((faz(), depois));
        }
    }
    None
}

/// Lê uma string CSS entre aspas a partir de `s`, devolvendo (conteúdo, resto).
///
/// Trata `\` como escape porque é assim que uma folha real escreve um caractere
/// que não consegue pôr no ficheiro: `content: "\2192"` é a seta que aparece nos
/// menus da Wikipédia. Sem isto, o utilizador via `2192` escrito na página.
pub(crate) fn string_css(s: &str) -> Option<(String, &str)> {
    let aspa = s.chars().next()?;
    let mut out = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        if c == aspa {
            return Some((out, &s[i + c.len_utf8()..]));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        // Escape: ou um código hexadecimal (até 6 dígitos, terminado por espaço
        // opcional), ou o caractere seguinte à letra.
        let hex: String = s[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .take(6)
            .collect();
        if hex.is_empty() {
            if let Some((_, lit)) = chars.next() {
                out.push(lit);
            }
            continue;
        }
        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
            out.push(ch);
        }
        // consome os dígitos e o espaço que os termina, se houver.
        let mut consumidos = hex.len();
        if s[i + 1 + hex.len()..].starts_with(' ') {
            consumidos += 1;
        }
        for _ in 0..consumidos {
            chars.next();
        }
    }
    None // string sem fecho — declaração inválida
}

/// Materializa o texto de um [`Content`] contra o elemento originante.
///
/// `contadores` é a fotografia dos contadores ativos nesta caixa, calculada em
/// ordem documental por [`crate::counters`]. `None` significa "esta página não
/// declara contadores" e não "o contador vale zero" — a diferença não se vê no
/// resultado (ambos dão o zero implícito da spec) mas vê-se no custo: sem
/// contadores na folha, a passagem documental não corre de todo.
///
/// `quotes` é a lista de pares HERDADA e efetiva do originante
/// ([`crate::dom::Dom::effective_quotes`]), e `profundidade_aspas` entra com
/// o nível calculado em ordem documental por [`crate::quotes::calcula`] e sai
/// mudado — quem chama para produzir texto de verdade e quem chama só para
/// avançar o nível global (`quotes::registra`) usam a MESMA função, para que
/// a escolha de qual par usar não tenha uma segunda resposta.
pub fn texto_de(
    content: &Content,
    attr: &impl Fn(&str) -> Option<String>,
    contadores: Option<&crate::counters::Snapshot>,
    quotes: &[(String, String)],
    profundidade_aspas: &mut i64,
) -> Option<String> {
    let Content::Pecas(pecas) = content else {
        return None;
    };
    let mut out = String::new();
    for p in pecas {
        match p {
            Peca::Texto(t) => out.push_str(t),
            // Atributo ausente dá string vazia (spec), NÃO cancela a caixa: uma
            // folha que escreve `content: "[" attr(x) "]"` ainda quer os
            // colchetes quando `x` não existe.
            Peca::Attr(nome) => out.push_str(&attr(nome).unwrap_or_default()),
            Peca::Contador(nome, estilo) => {
                out.push_str(&crate::counters::texto(contadores, nome, *estilo))
            }
            Peca::AbreAspas => out.push_str(&crate::quotes::abre(quotes, profundidade_aspas)),
            Peca::FechaAspas => out.push_str(&crate::quotes::fecha(quotes, profundidade_aspas)),
            Peca::NaoAbreAspas => crate::quotes::nao_abre(profundidade_aspas),
            Peca::NaoFechaAspas => crate::quotes::nao_fecha(profundidade_aspas),
        }
    }
    Some(out)
}

// Testes num FICHEIRO à parte (`pseudo/tests.rs`): este ficheiro chegou perto
// do tecto de 500 linhas do `CLAUDE.md` com a família de aspas
// (`open-quote`/`close-quote`), e a regra para um ficheiro nesse limite é
// módulo novo em vez de acréscimo. `pub(crate)` por causa do `textos` que
// `counters.rs` reusa — ver o comentário no próprio `tests.rs`.
#[cfg(test)]
pub(crate) mod tests;

