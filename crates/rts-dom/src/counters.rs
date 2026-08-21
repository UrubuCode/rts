//! CONTADORES DE CSS — `counter-reset`, `counter-increment` e o valor que
//! `counter()` lê dentro de `content`.
//!
//! ## Porque não é o mecanismo do `listitem.rs`
//!
//! O `listitem.rs` responde "qual é o meu número" DERIVANDO-o da posição do nó
//! entre os irmãos, e diz porquê: o layout deste motor mede subárvores fora de
//! ordem, e um contador acumulado veria a mesma lista duas vezes. Esse
//! argumento continua de pé aqui, e é ele que decide este desenho — mas a
//! resposta dele não serve para um contador de autor, e vale a pena dizer
//! exatamente onde deixa de servir: a numeração de lista é uma função da
//! POSIÇÃO (irmãos anteriores que também são item de lista), enquanto um
//! contador de CSS é uma função da ORDEM DOCUMENTAL INTEIRA — um
//! `counter-increment` num descendente de um irmão anterior conta, e nenhuma
//! varredura de irmãos o vê.
//!
//! O que se faz então é manter a PROPRIEDADE que o `listitem.rs` protege sem
//! copiar o mecanismo: os valores são calculados numa passagem única em ordem
//! documental, ANTES e FORA do layout, e ficam num memo por nó. Quem layouta
//! consulta uma tabela; nada acumula durante a travessia do layout, e por isso
//! medir a mesma subárvore duas vezes dá duas vezes a mesma resposta.
//!
//! O que É reusado do `listitem.rs` é a parte que responde à mesma pergunta:
//! [`crate::listitem::counter_text`] converte um número num sistema de
//! numeração (`decimal`, `lower-alpha`, romano). Escrever um segundo conversor
//! aqui daria duas respostas para "o que é o 27 em lower-alpha".
//!
//! ## O que fica de fora, e porquê
//!
//! `counters()` (plural, com separador) NÃO está implementado. Não é preguiça
//! nem juízo: o corpus de folhas reais deste repositório (`pagina.css`,
//! `google.css`, `wa.css`, `wa-app.css`) tem **zero** ocorrências —
//! oito `counter()` no singular e nenhuma no plural. Implementa-se o que as
//! páginas usam; uma pilha de escopos com separador serviria um caso que não
//! existe no corpus e precisaria de um teste inventado para se provar.
//!
//! O parser recusa `counters(` explicitamente, o que descarta a declaração
//! inteira e deixa a cascata continuar — em vez de a confundir com o singular e
//! pintar um número sem os antepassados.

use crate::style::ListStyleType;
use crate::{Dom, NodeIdx, NodeKind};

/// As operações de contador de um bloco de declarações.
///
/// Fica FORA do [`crate::style::ComputedStyle`] pela mesma razão que o
/// `content` fica: são duas listas de tamanho variável para servir umas dezenas
/// de elementos numa página de milhares, e o `ComputedStyle` é copiado por nó.
/// Vive ao lado das declarações, na regra.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ops {
    /// `counter-reset: a b 3` — cria o contador no escopo deste elemento com o
    /// valor dado (0 por omissão).
    pub reset: Vec<(String, i64)>,
    /// `counter-increment: a 2` — soma ao contador mais interno com esse nome
    /// (1 por omissão).
    pub increment: Vec<(String, i64)>,
}

impl Ops {
    pub fn is_empty(&self) -> bool {
        self.reset.is_empty() && self.increment.is_empty()
    }
}

/// Lê `counter-reset`/`counter-increment` do corpo CRU de uma regra.
///
/// Do corpo cru e não do bloco já parseado porque estas duas não são
/// propriedades do `ComputedStyle` — o parser de declarações descarta-as (ver
/// `style::inert`), e sempre as descartou; o que muda agora é que existe um
/// leitor. Devolve `None` quando o bloco não declara nenhuma das duas, para que
/// a regra não pague um `Vec` vazio.
pub fn parse_ops(body: &str) -> Option<Ops> {
    // Corte barato antes de dividir o corpo: numa folha real são umas dezenas de
    // regras em milhares, e este é o caminho por onde passam TODAS.
    if !body.contains("counter-") {
        return None;
    }
    let mut ops = Ops::default();
    for decl in crate::style::stylesheet::split_top_level_semicolons(body) {
        let Some((nome, valor)) = decl.split_once(':') else {
            continue;
        };
        let valor = valor.trim_end_matches("!important").trim();
        // Uma declaração POSTERIOR do mesmo nome substitui a anterior, como em
        // qualquer bloco CSS — não acumula.
        match nome.trim().to_ascii_lowercase().as_str() {
            "counter-reset" => ops.reset = parse_lista(valor, 0),
            "counter-increment" => ops.increment = parse_lista(valor, 1),
            _ => {}
        }
    }
    (!ops.is_empty()).then_some(ops)
}

/// `a b 3 c` → `[(a,omissao), (b,3), (c,omissao)]`.
///
/// O número é OPCIONAL e vem DEPOIS do nome; um nome sem número leva o valor de
/// omissão, que difere entre `reset` (0) e `increment` (1).
fn parse_lista(valor: &str, omissao: i64) -> Vec<(String, i64)> {
    if valor.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut out: Vec<(String, i64)> = Vec::new();
    for tok in valor.split_whitespace() {
        match tok.parse::<i64>() {
            // Um número só é o valor do nome que o precede; solto no início não
            // tem dono e é descartado.
            Ok(n) => {
                if let Some(ultimo) = out.last_mut() {
                    ultimo.1 = n;
                }
            }
            Err(_) => out.push((tok.to_string(), omissao)),
        }
    }
    out
}

/// A pilha de contadores ativos durante a travessia documental.
///
/// Cada entrada guarda a PROFUNDIDADE em que foi criada, que é o que permite
/// desfazer o escopo à saída do elemento: um `counter-reset` cria um contador
/// que vive no elemento, nos descendentes dele e nos irmãos seguintes — e é
/// precisamente essa última parte que impede um `HashMap` simples de servir.
#[derive(Default)]
struct Pilha {
    itens: Vec<(String, i64, usize)>,
}

impl Pilha {
    /// O valor do contador `nome` — o mais interno com esse nome.
    ///
    /// Um contador que ninguém criou vale 0: a spec diz que `counter()` sobre um
    /// nome sem `counter-reset` age como se a raiz o tivesse zerado. Devolver
    /// vazio perderia os literais à volta (`"[" counter(x) "]"`).
    ///
    /// Só os testes chamam: quem lê em produção lê a FOTOGRAFIA já tirada (ver
    /// [`texto`]), porque o `content` que cita o nome é conhecido depois desta
    /// passagem. Fica porque é aqui que a regra de escopo se prova.
    #[cfg(test)]
    fn valor(&self, nome: &str) -> i64 {
        self.itens
            .iter()
            .rev()
            .find(|(n, _, _)| n == nome)
            .map(|(_, v, _)| *v)
            .unwrap_or(0)
    }

    fn reset(&mut self, nome: &str, valor: i64, profundidade: usize) {
        // Um segundo `counter-reset` do mesmo nome NA MESMA profundidade
        // re-zera o que já existe em vez de empilhar — senão o `pop` da saída
        // deixaria um por desfazer.
        if let Some(e) = self
            .itens
            .iter_mut()
            .rev()
            .find(|(n, _, d)| n == nome && *d == profundidade)
        {
            e.1 = valor;
            return;
        }
        self.itens.push((nome.to_string(), valor, profundidade));
    }

    fn incrementa(&mut self, nome: &str, delta: i64, profundidade: usize) {
        match self.itens.iter_mut().rev().find(|(n, _, _)| n == nome) {
            Some(e) => e.1 += delta,
            // Incrementar um contador que não existe cria-o implicitamente a
            // partir de 0 (spec). É o caso de uma folha que só declara o
            // `counter-increment` e conta com o zero implícito da raiz.
            None => self.itens.push((nome.to_string(), delta, profundidade)),
        }
    }

    /// Desfaz o escopo de uma profundidade que se está a abandonar.
    fn sai(&mut self, profundidade: usize) {
        self.itens.retain(|(_, _, d)| *d < profundidade);
    }
}

/// Os valores visíveis a UMA caixa gerada: o par (nome, valor) de cada contador
/// ativo no momento em que o `content` dela é materializado.
///
/// Guarda-se a fotografia inteira e não só os nomes que aquele `content` cita
/// porque quem calcula (esta passagem) não conhece o `content` — quem o conhece
/// é o `pseudo_box`, que corre depois e por nó.
pub type Snapshot = std::rc::Rc<Vec<(String, i64)>>;

/// A tabela de contadores do documento: para cada `(elemento, pseudo)` que possa
/// gerar caixa, os valores visíveis a essa caixa.
pub type Tabela = crate::fasthash::FastMap<(NodeIdx, crate::style::PseudoElement), Snapshot>;

/// Percorre o documento em ordem e devolve a tabela.
///
/// `ops_de` responde as operações de um elemento ou de um dos seus
/// pseudo-elementos — é a cascata, injetada em vez de chamada, para que este
/// módulo não dependa do `Dom` inteiro nem os testes tenham de montar um.
pub fn calcula(
    dom: &Dom,
    ops_de: &dyn Fn(NodeIdx, Option<crate::style::PseudoElement>) -> Option<std::rc::Rc<Ops>>,
) -> Tabela {
    let mut tabela = Tabela::default();
    let mut pilha = Pilha::default();
    visita(dom, dom.root, 0, ops_de, &mut pilha, &mut tabela);
    tabela
}

fn visita(
    dom: &Dom,
    id: NodeIdx,
    profundidade: usize,
    ops_de: &dyn Fn(NodeIdx, Option<crate::style::PseudoElement>) -> Option<std::rc::Rc<Ops>>,
    pilha: &mut Pilha,
    tabela: &mut Tabela,
) {
    let elemento = matches!(dom.node(id).kind, NodeKind::Element { .. });
    if elemento {
        // A ORDEM aqui é a da spec e importa: todos os `reset` do elemento
        // primeiro, e só depois os `increment`. Ao contrário, um
        // `counter-reset:x; counter-increment:x` no mesmo bloco daria o valor do
        // reset em vez de reset+1.
        if let Some(ops) = ops_de(id, None) {
            for (n, v) in &ops.reset {
                pilha.reset(n, *v, profundidade);
            }
            for (n, d) in &ops.increment {
                pilha.incrementa(n, *d, profundidade);
            }
        }
        // O `::before` é o PRIMEIRO filho: as operações dele valem num escopo
        // próprio (profundidade+1) e são desfeitas antes dos filhos reais. É
        // exatamente o que a folha da Wikipédia precisa — o
        // `counter-increment` dos retrolinks está no `a::before`, não no `<a>`.
        pseudo(
            id,
            crate::style::PseudoElement::Before,
            profundidade + 1,
            ops_de,
            pilha,
            tabela,
        );
    }
    for &filho in &dom.node(id).children {
        visita(dom, filho, profundidade + 1, ops_de, pilha, tabela);
        pilha.sai(profundidade + 1);
    }
    if elemento {
        pseudo(
            id,
            crate::style::PseudoElement::After,
            profundidade + 1,
            ops_de,
            pilha,
            tabela,
        );
    }
}

fn pseudo(
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    profundidade: usize,
    ops_de: &dyn Fn(NodeIdx, Option<crate::style::PseudoElement>) -> Option<std::rc::Rc<Ops>>,
    pilha: &mut Pilha,
    tabela: &mut Tabela,
) {
    if let Some(ops) = ops_de(id, Some(pe)) {
        for (n, v) in &ops.reset {
            pilha.reset(n, *v, profundidade);
        }
        for (n, d) in &ops.increment {
            pilha.incrementa(n, *d, profundidade);
        }
    }
    // A fotografia é tirada para TODO elemento, tenha ele `content` ou não:
    // saber se tem exigiria correr a cascata do pseudo aqui, que é o trabalho
    // caro que o `pseudo_box` já faz e que esta passagem existe para não
    // duplicar. O custo é uma entrada de mapa por elemento numa página que
    // declara contadores — e nenhuma numa que não declare, porque a passagem
    // inteira não corre.
    if !pilha.itens.is_empty() {
        tabela.insert(
            (id, pe),
            std::rc::Rc::new(
                pilha
                    .itens
                    .iter()
                    .map(|(n, v, _)| (n.clone(), *v))
                    .collect(),
            ),
        );
    }
    pilha.sai(profundidade);
}

/// O texto de `counter(nome, estilo)` a partir de uma fotografia.
///
/// Reusa o conversor do `listitem.rs` — é a mesma pergunta ("o que é o 27 em
/// lower-alpha") e ter duas respostas para ela é o que esta casa já pagou.
pub fn texto(snapshot: Option<&Snapshot>, nome: &str, estilo: ListStyleType) -> String {
    let n = snapshot
        .and_then(|s| s.iter().rev().find(|(k, _)| k == nome))
        .map(|(_, v)| *v)
        .unwrap_or(0);
    crate::listitem::counter_text(estilo, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Os textos PINTADOS de uma página — o helper vive no `pseudo.rs` porque
    // é lá que a caixa gerada se resolve, e é reusado aqui em vez de copiado.
    // Os testes ponta a ponta dos contadores pertencem a este lado: o que eles
    // provam é a contagem, e é a contagem que mora neste ficheiro.
    use crate::pseudo::tests::textos;

    #[test]
    fn counter_reset_e_increment_numeram_as_caixas_geradas() {
        // O caso mínimo: um contador criado no contentor, incrementado em cada
        // filho, impresso pelo `::before` desse filho. É o que faz `1 2 3` — e
        // repare-se que o `counter-increment` está no ELEMENTO enquanto o
        // `counter()` está no pseudo dele.
        let t = textos(
            "<style>ul{counter-reset:n} li{counter-increment:n} \
             li::before{content:counter(n) \". \"}</style>\
             <ul><li>a</li><li>b</li><li>c</li></ul>",
        );
        let numeros: Vec<&String> = t.iter().filter(|s| s.contains('.')).collect();
        assert_eq!(
            numeros,
            // o espaço final do `content` é colapsado pelo fluxo inline, como
            // no browser — o que se fixa aqui é o NÚMERO, não o espaçamento.
            vec!["1.", "2.", "3."],
            "os três itens numeram em ordem documental: {t:?}"
        );
    }

    #[test]
    fn o_reset_reinicia_a_contagem_em_cada_lista() {
        // A prova de que o escopo do `counter-reset` existe: duas listas
        // irmãs contam 1,2 cada uma, e não 1,2,3,4. Sem escopo (um contador
        // global por nome) a segunda começaria no 3.
        let t = textos(
            "<style>ul{counter-reset:n} li{counter-increment:n} \
             li::before{content:counter(n)}</style>\
             <ul><li>a</li><li>b</li></ul><ul><li>c</li><li>d</li></ul>",
        );
        let n: Vec<String> = t
            .iter()
            .filter(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
            .cloned()
            .collect();
        assert_eq!(n, vec!["1", "2", "1", "2"], "{t:?}");
    }

    #[test]
    fn o_retrolink_de_citacao_multipla_da_a_b_c_como_na_wikipedia() {
        // O CASO REAL, com o markup e as regras que a folha da Wikipédia usa —
        // e o que estava a faltar: 152 letras isoladas que o Chrome pinta e nós
        // não. O `counter-increment` está no PRÓPRIO `::before`, o que obriga a
        // que as operações de um pseudo-elemento contem, e o `counter-reset`
        // está no `<span>` que os envolve, o que obriga a que o escopo do
        // pseudo (um filho) não vaze para o irmão seguinte errado.
        let t = textos(
            "<style>span[rel='mw:referencedBy']{counter-reset:mw-ref-linkback 0} \
             span[rel='mw:referencedBy'] > a::before\
             {counter-increment:mw-ref-linkback;content:counter(mw-ref-linkback,lower-alpha)}\
             </style>\
             <span rel='mw:referencedBy'><a></a><a></a><a></a></span>",
        );
        assert_eq!(t, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn a_regra_com_var_no_estilo_do_counter_perde_sem_apagar_a_que_ganha() {
        // As TRÊS regras de retrolink da folha da Wikipédia, verbatim e na ordem
        // do ficheiro. As duas primeiras não são cumpríveis aqui (o estilo vem
        // dentro de `var()`, que só resolve por elemento) e a terceira é a que o
        // Chrome também aplica, por ser a última de igual especificidade.
        //
        // O que este teste protege é a ORDEM em que se descarta: uma regra
        // recusada não pode apagar o `content` de outra, e uma recusada DEPOIS
        // não pode apagar o da que já tinha ganho. Sem isso, a folha real dava
        // caixa nenhuma apesar de o mecanismo de contadores funcionar — que é o
        // modo de falha mais caro, porque o teste sintético passa.
        let t = textos(
            "<style>\
             span[rel='mw:referencedBy']{counter-reset:mw-ref-linkback 0}\
             span[rel='mw:referencedBy'] > a::before{content:counter(mw-references,var(--cite-counter-style)) var(--cite-backlink-separator) counter(mw-ref-linkback,var(--cite-counter-style))}\
             span[rel='mw:referencedBy'] > a::before{counter-increment:mw-ref-linkback}\
             span[rel=\"mw:referencedBy\"] > a::before{font-weight:bold;font-style:italic;content:counter(mw-ref-linkback,lower-alpha)}\
             </style>\
             <span rel='mw:referencedBy'><a></a><a></a></span>",
        );
        assert_eq!(t, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn um_contador_que_ninguem_criou_vale_zero_e_nao_apaga_os_literais() {
        // A spec manda o zero implícito da raiz. O que aqui se fixa é a segunda
        // metade: os literais à volta sobrevivem. Devolver vazio faria o `[` e o
        // `]` desaparecerem com o número.
        let t = textos("<style>p::before{content:'[' counter(x) ']'}</style><p>oi</p>");
        assert_eq!(t[0], "[0]");
    }

    #[test]
    fn o_contador_do_ancestral_e_visivel_ao_pseudo_de_um_descendente() {
        // O outro contador dos retrolinks: `mw-references` é incrementado no
        // `<li>` e lido pelo `::before` de um `<a>` lá dentro, dois níveis
        // abaixo. Uma varredura de IRMÃOS — que é como o `listitem.rs` numera —
        // não responderia isto, e é a razão de este módulo existir.
        let t = textos(
            "<style>ol{counter-reset:r} li{counter-increment:r} \
             li a::before{content:counter(r)}</style>\
             <ol><li><span><a>x</a></span></li><li><span><a>y</a></span></li></ol>",
        );
        assert!(t.contains(&"1x".to_string()), "{t:?}");
        assert!(t.contains(&"2y".to_string()), "{t:?}");
    }


    #[test]
    fn a_lista_de_reset_leva_o_numero_do_nome_que_o_precede() {
        // A forma que a folha da Wikipédia escreve: três nomes sem número.
        assert_eq!(
            parse_lista("mw-ref-details-parent mw-references list-item", 0),
            vec![
                ("mw-ref-details-parent".into(), 0),
                ("mw-references".into(), 0),
                ("list-item".into(), 0),
            ]
        );
        // E a que leva número — incluindo negativo, que a mesma folha usa.
        assert_eq!(parse_lista("mw-ref-linkback -1", 0), vec![("mw-ref-linkback".into(), -1)]);
        assert_eq!(parse_lista("x", 1), vec![("x".into(), 1)]);
        assert_eq!(parse_lista("x 2 y", 1), vec![("x".into(), 2), ("y".into(), 1)]);
    }

    #[test]
    fn um_bloco_sem_contadores_nao_paga_vec_nenhum() {
        assert_eq!(parse_ops("color:red;font-size:2px"), None);
        assert_eq!(parse_ops("counter-reset:none"), None);
    }

    #[test]
    fn o_reset_de_um_elemento_alcanca_os_irmaos_seguintes_e_morre_no_pai() {
        // É a regra de escopo que impede um `HashMap` simples de servir: o
        // contador criado no primeiro filho ainda é visível no segundo.
        let mut p = Pilha::default();
        p.reset("x", 0, 1);
        p.incrementa("x", 1, 2);
        assert_eq!(p.valor("x"), 1);
        p.sai(2); // sai do primeiro filho
        assert_eq!(p.valor("x"), 1, "o irmão seguinte ainda vê o contador");
        p.sai(1); // sai do elemento que o criou
        assert_eq!(p.valor("x"), 0, "e fora dele volta ao zero implícito");
    }

    #[test]
    fn incrementar_um_contador_inexistente_conta_a_partir_de_zero() {
        let mut p = Pilha::default();
        p.incrementa("x", 1, 3);
        assert_eq!(p.valor("x"), 1);
    }
}
