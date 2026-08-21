//! `@supports (…)` — decidir se um bloco entra, em vez de o aplicar sempre.
//!
//! ## Porque não bastava ser transparente
//!
//! Era o único sítio do motor onde aplicávamos regras que o Chrome **não aplica
//! de todo**. Uma folha real escreve escadas assim:
//!
//! ```css
//! @supports not ((-webkit-mask-image:none) or (mask-image:none)) { … background-image … }
//! @supports     ((-webkit-mask-image:none) or (mask-image:none)) { … mask-image … }
//! ```
//!
//! Os dois ramos, o mesmo seletor, mutuamente exclusivos por construção. Aplicar
//! ambos não é "um pouco errado": é aplicar a alternativa que o autor escreveu
//! **para outro motor**, e deixá-la vencer ou perder por ordem no ficheiro em
//! vez de por capacidade. Na folha da Wikipédia são 76 blocos, 26 deles `not`.
//!
//! ## O que "suportamos" quer dizer aqui, e o que NÃO quer
//!
//! A pergunta do `@supports` é sobre uma DECLARAÇÃO, e a única resposta que este
//! motor pode dar sem inventar uma tabela nova é a que o próprio parser já dá:
//! **`aplica_declaracao` aceitou o par nome/valor?** Uma propriedade que o
//! `style::inert` recusa com motivo conta como NÃO suportada — é o que aquele
//! módulo existe para dizer.
//!
//! ⚠️ **Isto responde "sei ler", não "sei pintar".** Uma propriedade que
//! parseamos e guardamos sem nenhum consumidor responde `true` na mesma. É uma
//! resposta demasiado generosa e está dita aqui porque decide o resultado: no
//! par acima respondemos que suportamos `mask-image` — o que é verdade no
//! sentido do parser — e ficamos com o ramo da máscara, não com o do fundo.
//!
//! A alternativa seria responder "não" para as propriedades sem consumidor, e
//! foi recusada: a lista dessas propriedades não existe no código (é uma
//! varredura, não uma tabela), e escrevê-la à mão criaria uma segunda fonte da
//! mesma verdade — exatamente o que a regra da fonte única proíbe. Quando essa
//! lista existir gerada, é UMA linha aqui que muda.
//!
//! ## O que não é avaliado
//!
//! `selector(…)`, `font-tech(…)`, `font-format(…)` e a forma `@supports` seguido
//! de um identificador solto respondem **false**. Recusar é o lado seguro: o
//! ramo positivo é o do motor que suporta, e nós não sabemos responder.

use super::ComputedStyle;

/// `true` se o bloco `@supports <cond>` deve entrar.
///
/// `cond` é o texto entre `@supports` e o `{`.
pub(in crate::style::stylesheet) fn avalia(cond: &str) -> bool {
    // DESCONHECIDO conta como não — e a distinção entre "não" e "não sei" é o
    // ponto todo do tri-estado abaixo.
    or(&mut Cursor::novo(cond)) == Some(true)
}

/// Um cursor sobre a condição — descida recursiva com três níveis (`or`, `and`,
/// unário), a mesma forma do parser de `calc()` e pela mesma razão: a gramática
/// tem precedência e um `split` não a respeita.
struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
    texto: &'a str,
}

impl<'a> Cursor<'a> {
    fn novo(s: &'a str) -> Cursor<'a> {
        Cursor {
            s: s.as_bytes(),
            i: 0,
            texto: s,
        }
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    /// Consome a palavra `p` (sem distinguir caixa) se for a próxima. O teste do
    /// limite é o que impede `order` de ser lido como `or`.
    fn palavra(&mut self, p: &str) -> bool {
        self.ws();
        let fim = self.i + p.len();
        if fim <= self.s.len()
            && self.texto[self.i..fim].eq_ignore_ascii_case(p)
            && (fim == self.s.len() || !(self.s[fim] as char).is_alphanumeric())
        {
            self.i = fim;
            return true;
        }
        false
    }
}

/// `Some(true)`/`Some(false)` = sabemos; `None` = **não sabemos avaliar**.
///
/// O tri-estado existe por causa de um erro que o corpus apanhou: com um
/// booleano, `selector(…)` respondia `false` e `not selector(…)` respondia
/// `true` — ou seja, um `@supports not selector(:focus-visible)` ENTRAVA, quando
/// o Chrome (que suporta `:focus-visible`) o descarta. A negação transforma
/// "recuso por precaução" em "aceito por precaução", que é o contrário.
///
/// Com `None`, o desconhecido atravessa o `not` e o bloco cai dos dois lados.
/// São 3 blocos na folha da Wikipédia.
fn or(c: &mut Cursor) -> Option<bool> {
    let mut v = and(c);
    while c.palavra("or") {
        // O lado direito TEM de ser consumido na mesma, senão o cursor pára no
        // meio da condição e o resto é lido como lixo — daí não haver
        // curto-circuito aqui.
        let d = and(c);
        v = match (v, d) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        };
    }
    v
}

fn and(c: &mut Cursor) -> Option<bool> {
    let mut v = unario(c);
    while c.palavra("and") {
        let d = unario(c);
        v = match (v, d) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        };
    }
    v
}

fn unario(c: &mut Cursor) -> Option<bool> {
    if c.palavra("not") {
        return unario(c).map(|b| !b);
    }
    c.ws();
    if c.i < c.s.len() && c.s[c.i] == b'(' {
        let dentro = match equilibrio(&c.texto[c.i..]) {
            Some(n) => {
                let d = &c.texto[c.i + 1..c.i + n];
                c.i += n + 1;
                d
            }
            None => {
                c.i = c.s.len();
                return None;
            }
        };
        // Um parêntese ou é um grupo (`(a and b)`) ou é uma declaração
        // (`(color: red)`). A diferença é haver um `:` de topo.
        return match corta_dois_pontos(dentro) {
            Some((prop, val)) => Some(declaracao_suportada(prop.trim(), val.trim())),
            None => or(&mut Cursor::novo(dentro)),
        };
    }
    // `selector(…)`, `font-tech(…)`, um identificador solto: consome e responde
    // NÃO SEI. Responder "não" seria virar "sim" debaixo de um `not`.
    while c.i < c.s.len() && c.s[c.i] != b' ' {
        if c.s[c.i] == b'(' {
            match equilibrio(&c.texto[c.i..]) {
                Some(n) => c.i += n + 1,
                None => c.i = c.s.len(),
            }
            continue;
        }
        c.i += 1;
    }
    None
}

/// Índice do `)` que fecha o `(` da posição 0, contando aninhamento.
fn equilibrio(s: &str) -> Option<usize> {
    let mut d = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => d += 1,
            ')' => {
                d -= 1;
                if d == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Corta no primeiro `:` de NÍVEL 0 — o `:` dentro de `url(data:…)` ou de um
/// `(a: b)` aninhado não separa nada.
fn corta_dois_pontos(s: &str) -> Option<(&str, &str)> {
    let mut d = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => d += 1,
            ')' => d -= 1,
            ':' if d == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

/// A pergunta de fundo: o parser aceita este par?
///
/// Um estilo de rascunho e o mesmo `aplica_declaracao` que a cascade usa — em
/// vez de uma lista de nomes suportados, que seria uma segunda fonte da verdade
/// e dessincronizava no dia em que uma propriedade nova entrasse.
fn declaracao_suportada(prop: &str, val: &str) -> bool {
    if prop.is_empty() || val.is_empty() {
        return false;
    }
    let prop = prop.to_ascii_lowercase();
    // `inert` é a lista do que reconhecemos e deliberadamente não modelamos:
    // dizer que suportamos isso seria dizer o contrário do que aquele módulo diz.
    if crate::style::inert::is_inert(&prop) {
        return false;
    }
    // Duas perguntas, e a segunda foi descoberta por um teste a falhar: o
    // `aplica_declaracao` responde `true` quando reconhece o NOME, mesmo que o
    // VALOR não parseie — o braço `"color" => set_if(…, parse_color(val))` casa
    // e o `set_if` de um `None` não escreve nada. O Chrome responde `false` a
    // `(color: nao-e-uma-cor)`, e com razão: reconhecer o nome não é suportar a
    // declaração.
    //
    // A segunda pergunta é "mudou alguma coisa?", que a igualdade do
    // `ComputedStyle` responde sem uma tabela nova. Uma declaração cujo valor
    // calhe ser exatamente o default responde `false` por isto — é o lado
    // seguro: leva ao ramo de fallback, que é o que um motor mais fraco recebe.
    let mut rascunho = ComputedStyle::default();
    let reconhecido = crate::style::parse::aplica_declaracao(&mut rascunho, &prop, val);
    reconhecido && rascunho != ComputedStyle::default()
}

#[cfg(test)]
mod tests {
    use super::avalia;
    use crate::dom::parse_html_to_dom;

    /// Uma declaração que o parser aceita é suportada; uma que ele recusa não é.
    #[test]
    fn a_pergunta_de_fundo_e_o_parser() {
        assert!(avalia("(color: red)"));
        assert!(avalia("(display: flex)"));
        assert!(!avalia("(xpto-nao-existe: 1px)"));
        assert!(!avalia("(color: nao-e-uma-cor)"));
    }

    /// `not`, `and` e `or`, com a precedência da gramática e não a da ordem.
    #[test]
    fn os_operadores_respeitam_a_precedencia() {
        assert!(!avalia("not (color: red)"));
        assert!(avalia("not (xpto: 1px)"));
        assert!(avalia("(color: red) and (display: flex)"));
        assert!(!avalia("(color: red) and (xpto: 1px)"));
        assert!(avalia("(color: red) or (xpto: 1px)"));
        assert!(!avalia("(xpto: 1px) or (ypto: 2px)"));
        assert!(avalia("((color: red))"), "parênteses de grupo");
    }

    /// O que não sabemos responder é recusado, e não aceite por omissão: o ramo
    /// positivo é o do motor que suporta.
    #[test]
    fn o_que_nao_sabemos_avaliar_e_recusado() {
        // As DUAS formas caem, e é o ponto: com um booleano, a segunda entrava.
        assert!(!avalia("selector(:focus-visible)"));
        assert!(
            !avalia("not selector(:focus-visible)"),
            "o desconhecido tem de atravessar o `not`, senão a precaução inverte-se"
        );
        assert!(!avalia("(color: red) and selector(:x)"), "o and herda o não-sei");
        assert!(avalia("(color: red) or selector(:x)"), "mas um `true` do or decide");
        assert!(!avalia(""));
    }

    /// Uma propriedade que o `style::inert` recusa COM MOTIVO não é suportada —
    /// dizer o contrário contradizia o módulo que existe para o dizer.
    #[test]
    fn o_que_o_inert_recusa_nao_e_suportado() {
        assert!(crate::style::inert::is_inert("backdrop-filter"));
        assert!(!avalia("(backdrop-filter: blur(2px))"));
    }

    /// O caso que começou isto: os DOIS ramos de um par mutuamente exclusivo, o
    /// mesmo seletor, e antes disto os dois eram aplicados — o segundo vencia
    /// por vir depois no ficheiro, e não por o motor saber fazer o que ele pede.
    ///
    /// Agora entra UM. Aqui o par é decidível pelo parser (`float` sim, `xpto`
    /// não), que é a forma reduzida do `mask-image` da folha real.
    #[test]
    fn de_um_par_exclusivo_entra_so_um_ramo() {
        let dom = parse_html_to_dom(
            "<html><head><style>\
             @supports (xpto-nao-existe: 1px) { p { font-size: 11px } }\
             @supports not (xpto-nao-existe: 1px) { p { font-size: 22px } }\
             </style></head><body><p>x</p></body></html>",
        );
        let n = dom.query("p").unwrap();
        assert_eq!(
            dom.computed_property(n, "font-size"),
            "22px",
            "o ramo `not` é o verdadeiro, e o positivo não pode ter entrado"
        );
    }

    /// E a ordem no ficheiro deixa de decidir: com os ramos trocados, a resposta
    /// é a mesma. Antes desta mudança este teste dava o último dos dois.
    #[test]
    fn a_ordem_no_ficheiro_deixa_de_decidir() {
        let dom = parse_html_to_dom(
            "<html><head><style>\
             @supports not (xpto-nao-existe: 1px) { p { font-size: 22px } }\
             @supports (xpto-nao-existe: 1px) { p { font-size: 11px } }\
             </style></head><body><p>x</p></body></html>",
        );
        let n = dom.query("p").unwrap();
        assert_eq!(dom.computed_property(n, "font-size"), "22px");
    }

    /// O bloco suportado continua a entrar — a metade sem a qual isto seria
    /// pior que o defeito.
    #[test]
    fn o_bloco_suportado_continua_a_entrar() {
        let dom = parse_html_to_dom(
            "<html><head><style>@supports (display: flex) { p { font-size: 33px } }\
             </style></head><body><p>x</p></body></html>",
        );
        let n = dom.query("p").unwrap();
        assert_eq!(dom.computed_property(n, "font-size"), "33px");
    }
}
