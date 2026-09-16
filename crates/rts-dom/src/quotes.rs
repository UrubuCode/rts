//! ASPAS DE CONTEÚDO GERADO — `quotes`, `open-quote`, `close-quote`,
//! `no-open-quote`, `no-close-quote` (CSS2.1 §12.2).
//!
//! ## Duas perguntas, dois mecanismos — como em `counters.rs`
//!
//! A primeira pergunta é "que PARES de aspas valem aqui" — uma questão de
//! HERANÇA normal (`quotes` é herdável), respondida em
//! [`crate::dom::Dom::effective_quotes`] por uma subida de ancestrais: como
//! `quotes` fica fora do `ComputedStyle` (mesma razão do `content` — ver
//! [`crate::style::stylesheet::Rule::quotes`]), a herança da cascade não a
//! resolve sozinha, e subir a árvore por elemento é barato porque só corre
//! quando [`crate::style::Stylesheet::has_quote_content`] já filtrou que a
//! página usa `open-quote`/`close-quote`.
//!
//! A segunda é "em que PROFUNDIDADE de aninhamento estou" — e essa É a mesma
//! pergunta que os contadores respondem, pela mesma razão: a profundidade de
//! `open-quote` é uma função da ORDEM DOCUMENTAL inteira (cada abertura em
//! QUALQUER elemento anterior soma um nível, e o layout mede subárvores fora
//! de ordem). Por isso a profundidade também se calcula numa passagem única
//! em ordem documental, ANTES do layout, e fica num memo por (nó, pseudo) —
//! ver [`calcula`]. O que a torna DIFERENTE do `counters::Pilha` e por isso
//! um mecanismo à parte em vez de uma opção a mais na mesma pilha: a
//! profundidade de aspas é um ESCALAR ACHATADO sem escopo — nunca é desfeita
//! ao sair de um elemento (uma aspa aberta num filho continua aberta para o
//! irmão seguinte), enquanto cada contador é uma pilha NOMEADA que O É.
//! Threading os dois pelo mesmo `Pilha` obrigaria uma condicional por
//! operação para decidir qual das duas regras de saída de escopo aplicar —
//! mais acoplamento do que duas travessias curtas e sem estado partilhado.
//!
//! O que fica FORA de propósito: `quotes` declarado no PRÓPRIO
//! pseudo-elemento (`::before { quotes: … }`) não é lido — nenhuma folha do
//! corpus real faz isso, e a spec resolve-o pela herança normal do
//! originante de qualquer forma (o pseudo herda o `quotes` do elemento).

use crate::{Dom, NodeIdx, NodeKind};

/// Os pares (abre, fecha) declarados por um `quotes: "a" "b" "c" "d" …`, na
/// ordem de aninhamento (o primeiro par é o nível mais externo). `quotes:
/// none` dá um `Vec` vazio, que é diferente de "não declarado" (`None`, que
/// deixa o ancestral seguinte decidir) — [`crate::style::stylesheet::Rule`]
/// guarda os dois estados no mesmo `Option<Rc<Vec<_>>>`.
pub type Pares = Vec<(String, String)>;

/// Os pares TIPOGRÁFICOS por omissão do browser, usados quando nenhum
/// ancestral declara `quotes` — o par que os testes do WPT nunca precisam
/// (todos declaram `quotes` explicitamente), mas cuja ausência tornaria
/// `open-quote` sozinho, sem folha nenhuma, invisível em vez de "aspas
/// simples".
pub fn default_pares() -> Pares {
    vec![
        ("\u{201C}".to_string(), "\u{201D}".to_string()),
        ("\u{2018}".to_string(), "\u{2019}".to_string()),
    ]
}

/// Lê o valor de uma declaração `quotes: …` — pares de strings, ou `none`.
///
/// Reusa [`crate::pseudo::string_css`]: é o mesmo parser de string CSS que o
/// `content` usa, e duas cópias dele é o segundo mecanismo que este trabalho
/// existe para não criar. `None` descarta a declaração (número ímpar de
/// strings, ou algo que não é string nem `none`) e deixa a cascade continuar.
pub fn parse_quotes(valor: &str) -> Option<Pares> {
    let v = valor.trim();
    if v.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let mut pares = Vec::new();
    let mut resto = v;
    loop {
        resto = resto.trim_start();
        if resto.is_empty() {
            break;
        }
        let (abre, depois) = crate::pseudo::string_css(resto)?;
        let (fecha, depois) = crate::pseudo::string_css(depois.trim_start())?;
        pares.push((abre, fecha));
        resto = depois;
    }
    (!pares.is_empty()).then_some(pares)
}

/// Acha e parseia a declaração `quotes` do corpo CRU de uma regra — o mesmo
/// padrão de `parse_content_from_body` (a última declaração do bloco vence).
pub fn parse_quotes_from_body(body: &str) -> Option<Pares> {
    if !body.contains("quotes") {
        return None;
    }
    let mut achado = None;
    for decl in crate::style::stylesheet::split_top_level_semicolons(body) {
        let Some((nome, valor)) = decl.split_once(':') else {
            continue;
        };
        if !nome.trim().eq_ignore_ascii_case("quotes") {
            continue;
        }
        if let Some(q) = parse_quotes(valor.trim_end_matches("!important").trim()) {
            achado = Some(q);
        }
    }
    achado
}

/// Resolve um `open-quote`: o texto de abertura no nível `*profundidade`
/// (saturado no último par declarado, CSS2.1 §12.2), e incrementa o nível
/// para a próxima aspa. `pares` vazio (quer por `quotes:none`, quer por não
/// haver folha nenhuma) devolve string vazia sem deixar de contar o nível —
/// um `close-quote` a seguir ainda tem de desfazer este `open-quote`.
pub fn abre(pares: &[(String, String)], profundidade: &mut i64) -> String {
    let texto = par_em(pares, *profundidade)
        .map(|(a, _)| a.clone())
        .unwrap_or_default();
    *profundidade += 1;
    texto
}

/// Resolve um `close-quote`: desce o nível PRIMEIRO (clampado em 0 — fechar
/// mais aspas do que as abertas não gera profundidade negativa) e só depois
/// escolhe o par, porque o texto de fecho pertence ao nível que está a
/// terminar, não ao que veio antes de o abrir.
pub fn fecha(pares: &[(String, String)], profundidade: &mut i64) -> String {
    *profundidade = (*profundidade - 1).max(0);
    par_em(pares, *profundidade)
        .map(|(_, f)| f.clone())
        .unwrap_or_default()
}

/// `no-open-quote` — conta o nível sem inserir texto.
pub fn nao_abre(profundidade: &mut i64) {
    *profundidade += 1;
}

/// `no-close-quote` — desconta o nível sem inserir texto.
pub fn nao_fecha(profundidade: &mut i64) {
    *profundidade = (*profundidade - 1).max(0);
}

fn par_em(pares: &[(String, String)], profundidade: i64) -> Option<&(String, String)> {
    if pares.is_empty() {
        return None;
    }
    let i = (profundidade.max(0) as usize).min(pares.len() - 1);
    pares.get(i)
}

/// A profundidade de aspas ativa no INÍCIO da caixa gerada de cada
/// pseudo-elemento — o valor com que a primeira `open-quote`/`close-quote`
/// do `content` dele resolve. Ver o módulo para a razão de ser um mecanismo
/// à parte do [`crate::counters::Tabela`].
pub type Profundidades = crate::fasthash::FastMap<(NodeIdx, crate::style::PseudoElement), i64>;

/// Percorre o documento em ordem e devolve a tabela de profundidades.
///
/// `content_de` é a mesma pergunta que `pseudo_box` já responde
/// (`content` vencedor de um pseudo-elemento) — injetada em vez de chamada
/// para este módulo não depender do `Dom` inteiro nem os testes montarem um.
pub fn calcula(
    dom: &Dom,
    content_de: &dyn Fn(NodeIdx, crate::style::PseudoElement) -> Option<std::rc::Rc<crate::pseudo::Content>>,
) -> Profundidades {
    let mut tabela = Profundidades::default();
    let mut profundidade: i64 = 0;
    visita(dom, dom.root, content_de, &mut profundidade, &mut tabela);
    tabela
}

fn visita(
    dom: &Dom,
    id: NodeIdx,
    content_de: &dyn Fn(NodeIdx, crate::style::PseudoElement) -> Option<std::rc::Rc<crate::pseudo::Content>>,
    profundidade: &mut i64,
    tabela: &mut Profundidades,
) {
    let elemento = matches!(dom.node(id).kind, NodeKind::Element { .. });
    if elemento {
        registra(id, crate::style::PseudoElement::Before, content_de, profundidade, tabela);
    }
    for &filho in &dom.node(id).children {
        visita(dom, filho, content_de, profundidade, tabela);
    }
    if elemento {
        registra(id, crate::style::PseudoElement::After, content_de, profundidade, tabela);
    }
}

/// Regista a profundidade de ENTRADA desta caixa e avança o contador global
/// pelo `content` dela — chamando o MESMO [`crate::pseudo::texto_de`] que
/// produz o texto de verdade, com `quotes` vazio e o texto descartado: as
/// duas chamadas avançam `profundidade` identicamente porque `abre`/`fecha`
/// só usam a lista de pares para o TEXTO, nunca para o nível, e assim a
/// sequência de +1/-1 tem uma única implementação em vez de duas que
/// pudessem divergir.
fn registra(
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    content_de: &dyn Fn(NodeIdx, crate::style::PseudoElement) -> Option<std::rc::Rc<crate::pseudo::Content>>,
    profundidade: &mut i64,
    tabela: &mut Profundidades,
) {
    tabela.insert((id, pe), *profundidade);
    let Some(content) = content_de(id, pe) else {
        return;
    };
    let _ = crate::pseudo::texto_de(&content, &|_: &str| None, None, &[], profundidade);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_quote_usa_o_par_e_avanca_a_profundidade() {
        let pares = vec![("A".into(), "Z".into())];
        let mut p = 0i64;
        assert_eq!(abre(&pares, &mut p), "A");
        assert_eq!(p, 1);
    }

    #[test]
    fn close_quote_desce_antes_de_escolher_o_par() {
        // O par do NÍVEL QUE TERMINA, não do que veio antes de abrir — é a
        // ordem que faz `open open close close` com dois pares dar "PASS" e
        // não "PAPS" (quotes-applies-to-001 do WPT).
        let pares = vec![("P".into(), "S".into()), ("A".into(), "S".into())];
        let mut p = 0i64;
        let mut fora = String::new();
        fora.push_str(&abre(&pares, &mut p)); // "P", p=1
        fora.push_str(&abre(&pares, &mut p)); // "A", p=2
        fora.push_str(&fecha(&pares, &mut p)); // p=1, "S"
        fora.push_str(&fecha(&pares, &mut p)); // p=0, "S"
        assert_eq!(fora, "PASS");
        assert_eq!(p, 0);
    }

    #[test]
    fn close_quote_nao_desce_abaixo_de_zero() {
        let pares = vec![("A".into(), "Z".into())];
        let mut p = 0i64;
        assert_eq!(fecha(&pares, &mut p), "Z");
        assert_eq!(p, 0, "fechar sem ter aberto fica em zero, não em -1");
    }

    #[test]
    fn profundidade_alem_dos_pares_declarados_satura_no_ultimo() {
        let pares = vec![("A".into(), "Z".into())];
        let mut p = 5i64;
        assert_eq!(abre(&pares, &mut p), "A", "só há um par: satura nele");
    }

    #[test]
    fn quotes_none_da_vec_vazio_e_nao_apaga_como_declaracao_desconhecida() {
        assert_eq!(parse_quotes("none"), Some(Vec::new()));
        assert_eq!(parse_quotes(r#""A" "Z""#), Some(vec![("A".into(), "Z".into())]));
        assert_eq!(
            parse_quotes(r#""P" "S" "A" "S""#),
            Some(vec![("P".into(), "S".into()), ("A".into(), "S".into())])
        );
        // número ímpar de strings — declaração inválida, cascade continua.
        assert_eq!(parse_quotes(r#""A""#), None);
    }

    #[test]
    fn no_open_e_no_close_contam_sem_inserir_texto() {
        let mut p = 0i64;
        nao_abre(&mut p);
        assert_eq!(p, 1);
        nao_fecha(&mut p);
        assert_eq!(p, 0);
        nao_fecha(&mut p); // já em zero: fica em zero
        assert_eq!(p, 0);
    }
}
