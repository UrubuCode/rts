//! As LONGHANDS de `transition-*` e `animation-*`.
//!
//! Os shorthands `transition` e `animation` já existiam e já parseavam para
//! [`TransitionSpec`]/[`AnimationSpec`]; as longhands caíam no contador de
//! propriedade ignorada. Não é um detalhe de contagem: numa varredura das folhas
//! reais (`pagina.combinada.html`, `google.css`, `wa.css`) as treze longhands
//! somam ~660 declarações, mais do que os dois shorthands juntos — o CSS gerado
//! por ferramenta escreve quase sempre a forma longa. Uma folha que declara
//! `animation-name` + `animation-duration` em vez de `animation` não animava nada.
//!
//! ## O modelo tem UM spec por elemento, e por isso as longhands ACUMULAM
//!
//! `ComputedStyle` guarda `transition: Option<TransitionSpec>` — um valor, não
//! uma lista por propriedade. Então cada longhand LÊ o spec já presente e escreve
//! só o seu campo, criando um spec com os valores iniciais da spec CSS
//! (duração 0, delay 0, `ease`) quando ainda não há nenhum. Dentro de um bloco
//! isso dá o resultado certo em qualquer ordem, que é o caso que interessa.
//!
//! Entre REGRAS distintas não dá: o merge da cascade é por campo do
//! `ComputedStyle`, e o campo é o spec inteiro — uma regra mais específica que
//! declare só `transition-duration` substitui também o easing da regra anterior.
//! A alternativa correta é um campo por longhand (cinco campos em vez de um) e
//! recompor o spec no consumo; ficou de fora porque o consumidor está em `anim.rs`
//! e no loop do DOM, e reescrevê-lo é maior do que reconhecer as propriedades.
//! Fica dito aqui em vez de descoberto depois.
//!
//! ## Listas separadas por vírgula
//!
//! `transition-duration: .3s, .2s` declara tempos para duas propriedades. Como o
//! modelo transiciona `all` de uma vez, só o PRIMEIRO item é lido. Ignorar o
//! resto é o mesmo que o shorthand já fazia com o nome da propriedade.

use super::props::ComputedStyle;
use crate::anim::{AnimationSpec, Easing, TransitionSpec, parse_direction, parse_time_ms};

/// Os valores iniciais de `transition-*` da spec: duração 0 (= não transiciona),
/// sem delay, `ease`. Um spec de duração 0 é inerte — `progress` devolve 1 de
/// imediato —, portanto criar um só porque a folha declarou `transition-delay`
/// não liga transição nenhuma.
fn transition_or_initial(css: &ComputedStyle) -> TransitionSpec {
    css.transition.unwrap_or(TransitionSpec {
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::Ease,
    })
}

/// Idem para `animation-*`: nome vazio (nenhum `@keyframes` casa) e duração 0.
fn animation_or_initial(css: &ComputedStyle) -> AnimationSpec {
    css.animation.clone().unwrap_or(AnimationSpec {
        name: String::new(),
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::Ease,
        iterations: Some(1.0),
        direction: crate::anim::AnimDirection::Normal,
    })
}

/// O primeiro item de uma lista separada por vírgula (ver o cabeçalho do módulo).
///
/// A vírgula conta só no NÍVEL DE TOPO: `cubic-bezier(0.4, 0, 0.2, 1)` é UM item,
/// e um `split(',')` ingénuo devolvia `cubic-bezier(0.4` — o teste da curva foi
/// escrito antes desta função e apanhou-o.
fn first_item(val: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in val.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return val[..i].trim(),
            _ => {}
        }
    }
    val.trim()
}

/// Tenta aplicar `prop` como uma longhand (ou um alias `-webkit-`) de
/// `transition`/`animation`. Devolve `false` se o nome não é uma delas — e é isso
/// que o `parse` usa para decidir se conta a propriedade como ignorada.
///
/// UMA função em vez do par `is_longhand` + `apply` (o formato usado em
/// `borders`): ali a pergunta "é uma longhand?" é uma decomposição do nome que
/// serve às duas; aqui seriam duas listas de treze nomes para manter em sincronia,
/// e a lista repetida é exatamente o que costuma divergir.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // O prefixo de fornecedor é só um alias: `-webkit-transition` é `transition`.
    // Aceitá-los aqui evita treze braços a mais no `match` do parse.
    let name = prop
        .strip_prefix("-webkit-")
        .or_else(|| prop.strip_prefix("-moz-"))
        .unwrap_or(prop);
    match name {
        // Os shorthands prefixados — os sem prefixo têm braço próprio no parse e
        // nunca chegam aqui.
        "transition" => css.transition = TransitionSpec::parse(val),
        "animation" => css.animation = AnimationSpec::parse(val),

        "transition-duration" => {
            let Some(ms) = parse_time_ms(first_item(val)) else {
                return true;
            };
            let mut t = transition_or_initial(css);
            t.duration_ms = ms;
            css.transition = Some(t);
        }
        "transition-delay" => {
            let Some(ms) = parse_time_ms(first_item(val)) else {
                return true;
            };
            let mut t = transition_or_initial(css);
            t.delay_ms = ms;
            css.transition = Some(t);
        }
        // O valor INTEIRO vai para `Easing::parse`, não um token: o shorthand
        // parte por espaços e por isso perde `cubic-bezier(.4, 0, .2, 1)`, que tem
        // espaço depois das vírgulas. Pela longhand a curva chega inteira.
        "transition-timing-function" => {
            let Some(e) = Easing::parse(first_item(val)) else {
                return true;
            };
            let mut t = transition_or_initial(css);
            t.easing = e;
            css.transition = Some(t);
        }
        // `transition-property` só tem UM efeito que este modelo sabe honrar:
        // `none` desliga a transição. Os outros valores nomeiam propriedades, e o
        // modelo transiciona `all` — reconhecer o nome e guardar a lista num campo
        // que ninguém lê seria pior do que ignorá-la, porque parecia implementado.
        "transition-property" => {
            if val.trim().eq_ignore_ascii_case("none") {
                css.transition = None;
            }
        }

        "animation-name" => {
            let n = first_item(val);
            let mut a = animation_or_initial(css);
            // `none` é o inicial e não nomeia nenhum `@keyframes`.
            a.name = if n.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                n.to_string()
            };
            css.animation = Some(a);
        }
        "animation-duration" => {
            let Some(ms) = parse_time_ms(first_item(val)) else {
                return true;
            };
            let mut a = animation_or_initial(css);
            a.duration_ms = ms;
            css.animation = Some(a);
        }
        "animation-delay" => {
            let Some(ms) = parse_time_ms(first_item(val)) else {
                return true;
            };
            let mut a = animation_or_initial(css);
            a.delay_ms = ms;
            css.animation = Some(a);
        }
        "animation-timing-function" => {
            let Some(e) = Easing::parse(first_item(val)) else {
                return true;
            };
            let mut a = animation_or_initial(css);
            a.easing = e;
            css.animation = Some(a);
        }
        "animation-iteration-count" => {
            let v = first_item(val);
            let it = if v.eq_ignore_ascii_case("infinite") {
                None
            } else if let Ok(n) = v.parse::<f32>() {
                Some(n)
            } else {
                return true;
            };
            let mut a = animation_or_initial(css);
            a.iterations = it;
            css.animation = Some(a);
        }
        "animation-direction" => {
            let Some(d) = parse_direction(first_item(val)) else {
                return true;
            };
            let mut a = animation_or_initial(css);
            a.direction = d;
            css.animation = Some(a);
        }
        // RECONHECIDAS E INERTES, com o motivo. `AnimationSpec` não tem campo para
        // nenhuma das duas e o consumidor está fora deste módulo: `fill-mode`
        // decide o estilo ANTES e DEPOIS da animação (o loop do DOM aplica sempre o
        // computado) e `play-state: paused` exigiria um relógio por elemento.
        // Contá-las como ignoradas era pior: mandava implementar uma propriedade
        // cujo lugar é noutra camada.
        "animation-fill-mode" | "animation-play-state" => {}

        _ => return false,
    }
    true
}
