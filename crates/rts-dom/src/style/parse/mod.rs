//! Parse de DECLARAÇÕES CSS (`prop: valor; ...`) → [`ComputedStyle`]/[`DeclBlock`].
//! É o parser do `style=""` inline E do corpo `{ ... }` de cada regra de
//! stylesheet (reusado por `stylesheet.rs`). Shorthands (`margin`, `border`,
//! `font`, `gap`) expandem para os campos da tabela (`props.rs`) aqui — por isso o
//! dispatch nome→campo é um match explícito, não gerado (1 nome ≠ 1 campo).
//! Ignora propriedade/valor desconhecido sem panicar (robustez de parser real).

mod fundo_grelha;
mod caixa;
mod fluxo;

pub(in crate::style::parse) use super::color::parse_color;
pub(in crate::style::parse) use super::aplica::{set_edges, set_if, set_ou_limpa, set_side};
pub(in crate::style::parse) use super::lengths::{
    Caixa, parse_dimension, parse_dimension_min_max, parse_dimension_signed, parse_edges, parse_gap_pair,
    parse_inset, parse_len, parse_px, parse_side, parse_signed_px, split_top_ws,
};
pub(in crate::style::parse) use super::props::ComputedStyle;
pub(in crate::style::parse) use super::stylesheet::DeclBlock;
pub(in crate::style::parse) use super::values::{
    AlignItems, BorderStyle, Dimension, DisplayKind, Edges, FlexDirection, FlexWrap, FloatSide,
    JustifyContent, LineHeight, Position, Side, TextAlign, TextTransform, Visibility, WhiteSpace,
};

/// Parseia um `style="prop: valor; ..."` para um [`ComputedStyle`] (só a camada
/// NORMAL — atalho retrocompatível; `!important` inline é raro). Para a cascade
/// completa com `!important`, use [`parse_inline_block`].
///
/// Resolve as PENDENTES (`var()`, e desde o lote `flex-reverse-order` uma
/// `-inline-` lógica — `style::logical::e_direction_dependente`) contra um
/// contexto VAZIO: sem árvore, `parse_inline` não tem custom props nem
/// `direction` herdado nenhum para lhes dar, e é isso — não "fica por
/// resolver" — que um bloco solto, sem ancestral, responde (`direction`
/// cai no inicial `ltr`, como sempre respondeu antes deste lote). É a mesma
/// pergunta que `dom::cascade::computed_style_idx_inner` faz por elemento,
/// só que aqui o "elemento" não existe.
pub fn parse_inline(style: &str) -> ComputedStyle {
    let mut block = parse_inline_block(style);
    if block.pending.is_empty() {
        return block.normal;
    }
    let vars = std::collections::HashMap::new();
    for (prop, raw, important) in std::mem::take(&mut block.pending) {
        let target = if important { &mut block.important } else { &mut block.normal };
        super::stylesheet::apply_resolved_decl(target, &prop, &raw, &vars, None);
    }
    block.normal
}

/// Preserva as declarações de um `style="..."` no estado especificado. O
/// resultado ainda não contém valores computados nem herança; é a fronteira
/// pública para tooling e para futuras fases de resolução por propriedade.
pub fn parse_inline_specified(style: &str) -> crate::style::syntax::SpecifiedStyle {
    let source = format!("* {{{style}}}");
    let ast = crate::style::syntax::StylesheetAst::parse(&source);
    ast.items
        .iter()
        .find_map(|item| match item {
            crate::style::syntax::AstItem::QualifiedRule { block, .. } => {
                Some(crate::style::syntax::SpecifiedStyle::from_block(block))
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Parseia um bloco de declarações usando o AST sintáctico e baixa cada
/// declaração para o IR semântico existente. O wrapper sintético permite que
/// `style="..."` e corpos `{ ... }` partilhem exactamente a mesma gramática.
pub fn parse_inline_block(style: &str) -> DeclBlock {
    let source = format!("* {{{style}}}");
    let ast = crate::style::syntax::StylesheetAst::parse(&source);
    let Some(block) = ast.items.iter().find_map(|item| match item {
        crate::style::syntax::AstItem::QualifiedRule { block, .. } => Some(block),
        _ => None,
    }) else {
        return DeclBlock::default();
    };
    let mut lowered = DeclBlock::default();
    for declaration in block.declarations() {
        let value = declaration
            .value
            .iter()
            .map(crate::style::syntax::ComponentValue::to_css_semantic)
            .collect::<String>();
        apply_specified_declaration(
            &mut lowered,
            &declaration.name,
            &value,
            declaration.important,
        );
    }
    lowered
}

/// Parser semântico de uma declaração já isolada. Fica separado do entrypoint
/// AST para que lowering de regras e resolução de `var()` não criem ASTs
/// sintéticos recursivamente.
#[allow(dead_code)]
pub(crate) fn parse_inline_block_raw(style: &str) -> DeclBlock {
    let _phase = crate::metrics::phases::scope("parse-decls");
    let mut block = DeclBlock::default();
    // `split_top_level_semicolons` e nao `split(';')`: um `;` DENTRO de
    // parenteses ou aspas nao separa declaracoes. O caso que o mostrou e o
    // `url(data:image/png;base64,…)` — o split ingenuo cortava-o em
    // `url(data:image/png` e deixava `base64,…)` a ser lido como uma
    // declaracao propria. Medido no corpus: 134 `url()` com `;` la dentro, em
    // 7 folhas, e ZERO data-URI chegava inteiro a um elemento.
    //
    // O partidor certo ja existia ao lado, usado pelo corpo das regras e pelos
    // contadores; era so este caminho que nao o usava.
    for decl in crate::style::stylesheet::split_top_level_semicolons(style) {
        let mut it = decl.splitn(2, ':');
        let (prop, val_raw) = match (it.next(), it.next()) {
            // ASSIMETRIA DELIBERADA: o nome de uma propriedade normal é
            // case-insensitive (`COLOR` = `color`) — é para isso que a
            // minusculação existe — mas o nome de uma CUSTOM PROPERTY é
            // case-SENSITIVE por spec (CSS Variables §2), logo `--A` e `--a`
            // são duas variáveis. Minusculá-lo em bloco gravava `--Mhs7de` como
            // `--mhs7de`, e o `var(--Mhs7de)` — que vive no VALOR, e o valor
            // nunca é minusculado — não encontrava nada: a declaração inteira
            // caía. No `google.css` são 80 dos 91 nomes, incluindo o
            // `body{font-size:var(--Mhs7de)}` de que todo o documento herda.
            // Não uniformizar isto de volta.
            (Some(p), Some(v)) => {
                let p = p.trim();
                let p = if p.starts_with("--") {
                    p.to_string()
                } else {
                    p.to_ascii_lowercase()
                };
                (p, v.trim())
            }
            _ => continue,
        };
        crate::bump!(css_declarations);
        // Destaca o sufixo `!important` (case-insensitive) do valor; a camada de
        // destino depende dele.
        let (val, important) = split_important(val_raw);
        // CUSTOM PROPERTY (`--nome: valor`): guarda o valor CRU no bloco — a
        // cascade por elemento resolve (#1779). Importância ignorada (v1).
        if prop.starts_with("--") {
            crate::bump!(css_custom_declarations);
            if important {
                block.custom_important.push((prop.clone(), val.to_string()));
            } else {
                block.custom.push((prop.clone(), val.to_string()));
            }
            continue;
        }
        // Valor com `var()`: NÃO parseia agora — vira declaração PENDENTE, que a
        // cascade resolve POR ELEMENTO (contra as custom props dele) na posição
        // desta regra.
        if val.contains("var(") {
            crate::bump!(css_var_refs);
            block
                .pending
                .push((prop.clone(), val.to_string(), important));
            continue;
        }
        // `inherit` — vale para QUALQUER propriedade e não se parece com nenhum
        // valor: guarda-se o NOME, e a passada de herança copia o campo do pai
        // (ver `style::inherit_kw`). Antes disto, a declaração era descartada em
        // silêncio, o que deixava vencer uma regra menos específica.
        let css = if important {
            &mut block.important
        } else {
            &mut block.normal
        };
        if val.eq_ignore_ascii_case("inherit") {
            let mut nomes = css.inherit_props.as_deref().cloned().unwrap_or_default();
            if !nomes.contains(&prop) {
                nomes.push(prop.clone());
            }
            set_if(&mut css.inherit_props, Some(std::sync::Arc::new(nomes)));
            continue;
        }
        // ÚLTIMA TENTATIVA: tirar o prefixo de fornecedor e repetir.
        //
        // 16 nomes do corpus são `-webkit-box-shadow`, `-o-object-fit`,
        // `-moz-column-gap`, `-ms-transform` e companhia — o mesmo nome com um
        // prefixo, o mesmo valor, e a versão nua já implementada. Um braço
        // literal por cada seria dezasseis linhas a repetir dezasseis decisões
        // já tomadas.
        //
        // É a ÚLTIMA e não a primeira de propósito, e é isso que a torna segura.
        // Tudo o que trata o prefixado de maneira própria já correu na primeira
        // chamada: `vocab` e `painting` cortam o prefixo eles mesmos, e
        // `inert::is_inert` também — é lá que estão as duas sintaxes ANTIGAS de
        // flexbox (`-webkit-box-flex`, `-ms-flex-pack`), que NÃO são aliases e
        // que uma tentativa cega traduziria para os campos errados em silêncio.
        // Chegar aqui já é prova de que nenhum caminho reclamou o nome.
        //
        // Só uma vez: `strip_prefix` não recorre, portanto não há ciclo.
        if !aplica_declaracao(css, prop.as_str(), val) {
            let nu = prop
                .strip_prefix("-webkit-")
                .or_else(|| prop.strip_prefix("-moz-"))
                .or_else(|| prop.strip_prefix("-ms-"))
                .or_else(|| prop.strip_prefix("-o-"));
            match nu {
                // A MESMA função, com o nome sem prefixo — e não uma segunda
                // lista de aliases. Toda a cadeia (`vocab`, `painting`,
                // `timing`, `radius`, `grid_lines`, `logical`, `inert`) está lá
                // dentro, portanto o nome nu é resolvido exatamente como se a
                // folha o tivesse escrito assim.
                Some(n) if aplica_declaracao(css, n, val) => {}
                _ => {
                    crate::bump!(css_declarations_unknown);
                    crate::note!("propriedade-ignorada", prop.clone());
                }
            }
        }
    }
    block
}

fn is_css_wide_keyword(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "initial" | "inherit" | "unset" | "revert" | "revert-layer"
    )
}

/// Aplica `initial`/`inherit`/`unset`/`revert`/`revert-layer` à camada
/// semântica. Os valores iniciais são convertidos pelo mesmo dispatch das
/// propriedades normais, em vez de duplicar os tipos na tabela `initial`.
/// `revert`/`revert-layer` NÃO resolvem aqui — uma declaração isolada não tem
/// acesso às regras casadas (não sabe de que origem/layer veio, nem qual foi a
/// anterior): guardam só o NOME em `revert_props`/`revert_layer_props`, o
/// mesmo padrão de `inherit_props` acima, e a resolução real acontece em
/// `style::stylesheet::revert::resolve_reverts` — chamada por
/// `declarations_from` depois de ter a lista ordenada de regras na mão
/// (lote J; antes disto o par era descartado em silêncio aqui mesmo).
pub(crate) fn apply_css_wide_keyword(
    css: &mut ComputedStyle,
    prop: &str,
    value: &str,
) -> bool {
    let keyword = value.trim().to_ascii_lowercase();
    match keyword.as_str() {
        "inherit" => {
            crate::style::inherit_kw::clear_inherit_marker(css, prop);
            let mut nomes = css.inherit_props.as_deref().cloned().unwrap_or_default();
            if !nomes.iter().any(|name| name == prop) {
                nomes.push(prop.to_string());
            }
            css.inherit_props = Some(std::sync::Arc::new(nomes));
            true
        }
        "unset" if is_inherited_property(prop) => {
            apply_css_wide_keyword(css, prop, "inherit")
        }
        "unset" | "initial" => {
            let applied = apply_initial_value(css, prop);
            if applied {
                crate::style::inherit_kw::clear_inherit_marker(css, prop);
                crate::style::inherit_kw::mark_initial_property(css, prop);
            }
            applied
        }
        "revert" => {
            let mut nomes = css.revert_props.as_deref().cloned().unwrap_or_default();
            if !nomes.iter().any(|name| name == prop) {
                nomes.push(prop.to_string());
            }
            css.revert_props = Some(std::sync::Arc::new(nomes));
            true
        }
        "revert-layer" => {
            let mut nomes = css
                .revert_layer_props
                .as_deref()
                .cloned()
                .unwrap_or_default();
            if !nomes.iter().any(|name| name == prop) {
                nomes.push(prop.to_string());
            }
            css.revert_layer_props = Some(std::sync::Arc::new(nomes));
            true
        }
        _ => false,
    }
}

fn is_inherited_property(prop: &str) -> bool {
    matches!(
        prop,
        "color"
            | "font"
            | "font-size"
            | "font-family"
            | "font-style"
            | "font-weight"
            | "font-stretch"
            | "line-height"
            | "text-align"
            | "text-decoration"
            | "text-decoration-line"
            | "letter-spacing"
            | "word-spacing"
            | "white-space"
            | "text-transform"
            | "visibility"
            | "direction"
            | "tab-size"
            | "word-break"
            | "overflow-wrap"
            | "word-wrap"
            | "text-indent"
            | "list-style-type"
            | "list-style-position"
            | "pointer-events"
            | "caret-color"
            | "hyphens"
            | "line-break"
            | "text-wrap"
    )
}

fn apply_initial_value(css: &mut ComputedStyle, prop: &str) -> bool {
    // A inicialização dos shorthands abaixo é válida para todas as suas
    // longhands modeladas e passa pelo parser normal para conservar os clears.
    if prop == "font-family" {
        css.font_family = None;
        return true;
    }
    let value = match prop {
        "margin" | "padding" => Some("0px"),
        "border" => Some("medium none black"),
        "display" => Some("inline"), // initial CSS de display; o default da tag é UA.
        _ => crate::style::initial::initial(prop),
    };
    let Some(value) = value else {
        return false;
    };
    aplica_declaracao(css, prop, value)
}

/// Aplica uma declaração já extraída pelo AST sobre o mesmo `DeclBlock`, sem
/// criar um snapshot temporário. O parser textual continua separado para
/// compatibilidade, mas regras e inline AST usam esta operação para conservar a
/// ordem dos shorthands/longhands e os efeitos acumulativos do bloco.
pub(crate) fn apply_specified_declaration(
    block: &mut DeclBlock,
    prop_raw: &str,
    val: &str,
    important: bool,
) {
    let prop = prop_raw.trim();
    if prop.is_empty() {
        return;
    }
    let val = val.trim();
    let prop = if prop.starts_with("--") {
        prop.to_string()
    } else {
        prop.to_ascii_lowercase()
    };
    crate::bump!(css_declarations);

    if prop == "all" {
        if val.eq_ignore_ascii_case("initial") {
            if important {
                block.all_initial_important = true;
                block.important = ComputedStyle::default();
            } else {
                block.all_initial_normal = true;
                block.normal = ComputedStyle::default();
            }
        }
        return;
    }

    if prop.starts_with("--") {
        crate::bump!(css_custom_declarations);
        if important {
            block.custom_important.push((prop, val.to_string()));
        } else {
            block.custom.push((prop, val.to_string()));
        }
        return;
    }
    // `var()` adia para resolver contra as custom props do ELEMENTO; uma
    // logical `-inline-` (margin/padding/border/inset, `style::logical::
    // e_direction_dependente`) adia pela MESMA razão de fundo — o lado
    // físico depende de `direction`, que também só se conhece por elemento,
    // nunca no momento em que esta regra é compilada (`style::logical`,
    // cabeçalho "Quando isto resolve"). A mesma fila, os dois motivos —
    // `css_var_refs` só conta o motivo que já tinha nome.
    if val.contains("var(") {
        crate::bump!(css_var_refs);
        block
            .pending
            .push((prop, val.trim().to_string(), important));
        return;
    }
    if crate::style::logical::e_direction_dependente(&prop) {
        block
            .pending
            .push((prop, val.trim().to_string(), important));
        return;
    }
    apply_declaration_final(block, &prop, val, important);
}

/// A parte de baixo de [`apply_specified_declaration`]: já sabe que `prop`
/// não é `--custom`, `all` nem precisa de adiar — aplica de vez.
///
/// À parte para [`crate::style::stylesheet::apply_resolved_decl`] chamar
/// DIRECTO: uma declaração pendente já resolvida (var() substituído,
/// `direction` decidido) que voltasse a `apply_specified_declaration`
/// bateria nos MESMOS dois motivos de adiar e desapareceria em silêncio —
/// ver o comentário lá.
pub(crate) fn apply_declaration_final(block: &mut DeclBlock, prop: &str, val: &str, important: bool) {
    let css = if important {
        &mut block.important
    } else {
        &mut block.normal
    };
    if is_css_wide_keyword(val) && apply_css_wide_keyword(css, prop, val) {
        return;
    }

    crate::style::inherit_kw::clear_inherit_marker(css, prop);
    crate::style::inherit_kw::clear_initial_marker(css, prop);
    if !aplica_declaracao(css, prop, val) {
        let nu = prop
            .strip_prefix("-webkit-")
            .or_else(|| prop.strip_prefix("-moz-"))
            .or_else(|| prop.strip_prefix("-ms-"))
            .or_else(|| prop.strip_prefix("-o-"));
        match nu {
            Some(n) if aplica_declaracao(css, n, val) => {}
            _ => {
                crate::bump!(css_declarations_unknown);
                crate::note!("propriedade-ignorada", prop.to_string());
            }
        }
    }
}

/// Aplica `text-decoration` / `text-decoration-line`. `com_cor` distingue os
/// dois: o SHORTHAND também traz a cor (`underline dotted red`), e o parser da
/// linha já ignora os tokens que não são de linha — por isso a cor não tem onde
/// ser lida a não ser aqui. `-line` não aceita cor, mas nenhum valor de linha
/// parseia como cor, então partilhar o corpo não engana nenhum dos dois.
///
/// É `pub(super)` porque `style::vocab` a chama para as grafias prefixadas
/// (`-webkit-text-decoration`, 6 folhas do corpus), que nunca chegam ao `match`
/// deste ficheiro — ele casa por literal e não vê o prefixo. Uma segunda cópia
/// lá seria duas respostas à mesma pergunta, com a cor a ser lida só numa delas.
/// O SHORTHAND também traz `-style`/`-thickness` (`underline dotted red 2px`),
/// e nenhum outro sítio os lê dele — a longhand `text-decoration-style`/
/// `-thickness` continua a entrar por `style::painting::try_apply`, que já as
/// tinha; o que faltava era o shorthand preenchê-las quando as escreve JUNTO
/// da linha, em vez de deixá-las no valor anterior/inicial. `-line` não aceita
/// nenhum dos dois, por isso ficam atrás do mesmo `com_cor`.
pub(super) fn apply_text_decoration(css: &mut ComputedStyle, val: &str, com_cor: bool) {
    set_if(&mut css.text_decoration, crate::style::values::TextDecoration::parse(val));
    if com_cor {
        if let Some(c) = val.split_whitespace().find_map(parse_color) {
            set_if(&mut css.text_decoration_color, Some(c));
        }
        if let Some(s) = val
            .split_whitespace()
            .find_map(crate::style::painting::TextDecorationStyle::parse)
        {
            set_if(&mut css.text_decoration_style, Some(s));
        }
        if let Some(t) = val.split_whitespace().find_map(|tok| {
            if tok.eq_ignore_ascii_case("auto") {
                None
            } else {
                super::lengths::parse_inset(tok)
            }
        }) {
            set_if(&mut css.text_decoration_thickness, Some(t));
        }
    }
}

/// Aplica UMA declaração já normalizada. `false` = nenhum braço reclamou o
/// nome, e é o chamador que decide o que fazer com isso.
///
/// Extraída do corpo do laço para poder ser chamada DUAS vezes: uma com o nome
/// como veio, outra sem o prefixo de fornecedor. Enquanto o `match` estava
/// inline não havia como repetir a tentativa sem repetir os braços — e a
/// alternativa, uma segunda lista com os nomes que são alias puro, era mais uma
/// coisa a dessincronizar da primeira.
pub(super) fn aplica_declaracao(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "transform" => set_ou_limpa(&mut css.transform, val, crate::style::effects::Transform::parse(val)),
        "aspect-ratio" => set_if(&mut css.aspect_ratio, parse_aspect_ratio(val)),
        "opacity" => {
            // `opacity: <0..1>` (clampa fora do intervalo, como o browser).
            css.opacity = val.trim().parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
        }
        "font-size" => set_if(&mut css.font_size, parse_dimension(val)),
        "font-weight" => set_if(&mut css.bold, Some(is_bold(val))),
        "font-style" => {
            css.italic =
                Some(val.eq_ignore_ascii_case("italic") || val.eq_ignore_ascii_case("oblique"))
        }
        // ── Texto/fonte (#1749) ────────────────────────────────────────────────
        "text-align" => set_if(&mut css.text_align, TextAlign::parse(val)),
        "line-height" => set_if(&mut css.line_height, LineHeight::parse(val)),
        "white-space" => set_if(&mut css.white_space, WhiteSpace::parse(val)),
        "visibility" => set_if(&mut css.visibility, Visibility::parse(val)),
        "text-transform" => set_if(&mut css.text_transform, TextTransform::parse(val)),
        "letter-spacing" => {
            // `normal` = 0; senão um comprimento (px/em/rem — resolve p/ px cedo
            // seria ideal, mas letter-spacing quase sempre vem em px/em pequenos;
            // usa parse_len que cobre px). `normal`/inválido → None.
            // `normal` = 0. NEGATIVO é legal e usa-se para apertar títulos
            // (`letter-spacing: -1px`); o `parse_len` recusa-o por servir
            // larguras, daí o caminho com sinal.
            css.letter_spacing = if val.trim().eq_ignore_ascii_case("normal") {
                Some(0.0)
            } else {
                parse_signed_px(val)
            };
        }
        "text-decoration" | "text-decoration-line" => {
            apply_text_decoration(css, val, prop != "text-decoration-line")
        }
        "font-family" => set_if(&mut css.font_family, parse_font_family(val)),
        "font" => apply_font_shorthand(css, val),
        // ── overflow (#1744): scroll container interno. `overflow` define os dois
        // eixos; `-x`/`-y` cada um. Reusa o enum do módulo scrollbar.
        "overflow" => {
            let o = crate::scrollbar::Overflow::parse_str(val);
            css.overflow_x = o;
            css.overflow_y = o;
        }
        "overflow-x" => set_if(&mut css.overflow_x, crate::scrollbar::Overflow::parse_str(val)),
        "overflow-y" => set_if(&mut css.overflow_y, crate::scrollbar::Overflow::parse_str(val)),
        // ── Box model: shorthand 1/2/3/4 valores + longhands por lado. ─────────
        "transition" => set_if(&mut css.transition, crate::anim::TransitionSpec::parse(val)),
        "animation" => set_if(&mut css.animation, crate::anim::AnimationSpec::parse(val)),
        // Uma propriedade que nenhum braço reconhece é CSS que a página
        // escreveu e o motor ignora em silêncio. Contá-la é o que transforma
        // "o layout não bate com o Chrome" numa lista de nomes a implementar.
        // GRUPOS de propriedades resolvidos por módulo, antes de desistir. Um
        // grupo aqui em vez de treze braços literais mantém a lista de nomes
        // do lado de quem os aplica — uma segunda lista neste `match` era o
        // sítio óbvio para uma delas ficar de fora.
        _ if fundo_grelha::try_apply(css, prop, val) => {}
        _ if caixa::try_apply(css, prop, val) => {}
        _ if fluxo::try_apply(css, prop, val) => {}
        _ if crate::style::timing::try_apply(css, &prop, val) => {}
        _ if crate::style::logical::try_apply(css, &prop, val) => {}
        _ if crate::style::vocab::try_apply(css, &prop, val) => {}
        _ if crate::style::radius::try_apply(css, &prop, val) => {}
        _ if crate::style::grid_lines::try_apply(css, &prop, val) => {}
        _ if crate::style::painting::try_apply(css, &prop, val) => {}
        // RECONHECIDA e deliberadamente não modelada: conta noutra coluna,
        // para a coluna das desconhecidas continuar a ser a lista do que
        // falta fazer e não uma mistura com o que foi recusado.
        _ if crate::style::inert::is_inert(&prop) => {
            crate::bump!(css_declarations_inert);
        }
        _ => return false,
    }
    true
}

/// Separa o sufixo `!important` (case-insensitive, com espaços) de um valor CSS.
/// Devolve `(valor_sem_important, é_important)`. `"red !important"` → `("red", true)`.
#[allow(dead_code)]
fn split_important(val: &str) -> (&str, bool) {
    let v = val.trim();
    // Acha `!important` no fim, tolerante a espaço entre `!` e `important` não — a
    // spec exige `!important` colado (espaço só antes do `!`).
    let lower = v.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix("!important") {
        let cut = stripped.len();
        return (v[..cut].trim_end(), true);
    }
    (v, false)
}

/// Parseia `display: block|flex|inline|inline-block|none` para [`DisplayKind`].
/// Extrai o Nº DE COLUNAS de `grid-template-columns`: de `repeat(N, ...)` pega N; de
/// uma lista de trilhas (`1fr 1fr 1fr`, `200px 200px`) conta os itens de topo. `None`
/// para valores que não dão um número (auto/subgrid/…). Cobre o padrão Tailwind
/// `grid-cols-N` (= `repeat(N, minmax(0,1fr))`).
fn parse_grid_columns(v: &str) -> Option<i32> {
    let v = v.trim();
    let low = v.to_ascii_lowercase();
    if let Some(i) = low.find("repeat(") {
        let inner = &v[i + "repeat(".len()..];
        // o 1º argumento (antes da 1ª vírgula de topo) é a contagem.
        let count = inner.split(',').next()?.trim();
        return count.parse::<i32>().ok().filter(|n| *n >= 1);
    }
    // lista de trilhas separadas por espaço de TOPO (respeita parênteses de minmax()).
    let n = split_top_ws(v).len() as i32;
    (n >= 1).then_some(n)
}

/// Parseia `aspect-ratio`: `<w> / <h>` (ex. `16 / 9`) ou um número único (`1.5`).
/// `auto`/inválido → `None`. Devolve a razão largura/altura.
fn parse_aspect_ratio(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some((w, h)) = v.split_once('/') {
        let w = w.trim().parse::<f32>().ok()?;
        let h = h.trim().parse::<f32>().ok()?;
        return (h != 0.0 && w > 0.0).then_some(w / h);
    }
    v.parse::<f32>().ok().filter(|r| *r > 0.0)
}

/// Valores não suportados (table, …) → `None` (cai no default da tag).
fn parse_display(v: &str) -> Option<DisplayKind> {
    match v.trim().to_ascii_lowercase().as_str() {
        // `flow-root` computa como `block` NA CAIXA; o que a distingue vive no
        // campo `flow_root`, levantado pelo braço de `display` — aqui não há
        // `css` à mão. Ver `style/props/tabela.rs`.
        "block" | "flow-root" => Some(DisplayKind::Block),
        "flex" => Some(DisplayKind::Flex),
        // Variante PRÓPRIA — não `Flex` — pela mesma razão de `inline-block`
        // ao lado: ver `style/values/display.rs::DisplayKind::InlineFlex`.
        "inline-flex" => Some(DisplayKind::InlineFlex),
        "inline" => Some(DisplayKind::Inline),
        // `inline-block` tem variante PRÓPRIA desde que ela existe: colapsá-la em
        // `Inline` fazia o computed responder `inline` onde o browser responde
        // `inline-block` (8 desvios do corpus). Para o LAYOUT continua a valer o
        // mesmo código — `DisplayKind::to_display_code` mapeia as duas no mesmo —,
        // portanto isto corrige a resposta sem mudar a disposição.
        "inline-block" => Some(DisplayKind::InlineBlock),
        "grid" | "inline-grid" => Some(DisplayKind::Grid),
        "none" => Some(DisplayKind::None),
        // `list-item` — o `<li>`. Bloco MAIS um marcador; ver `crate::listitem`.
        "list-item" => Some(DisplayKind::ListItem),
        // Os valores de TABELA. `inline-table` cai em `Table` porque a diferença
        // é só como a caixa participa do fluxo do PAI (inline vs bloco), e por
        // dentro é a mesma repartição de colunas; tratá-lo como caixa inline é
        // um refino, não um algoritmo à parte.
        "table" | "inline-table" => Some(DisplayKind::Table),
        "table-row-group" | "table-header-group" | "table-footer-group" => {
            Some(DisplayKind::TableRowGroup)
        }
        "table-row" => Some(DisplayKind::TableRow),
        "table-cell" => Some(DisplayKind::TableCell),
        "table-caption" => Some(DisplayKind::TableCaption),
        // `table-column`/`table-column-group` (`<col>`/`<colgroup>`) NÃO geram
        // caixa nenhuma no CSS — só carregam largura para as colunas. Devolver
        // `None` aqui os faria cair no default da tag (bloco) e pintar uma caixa
        // vazia que o Chrome não tem; `None` (o display) é o que os apaga.
        "table-column" | "table-column-group" => Some(DisplayKind::None),
        _ => None,
    }
}

/// Aplica o shorthand `border: <width> <style> <color>` — os 3 em QUALQUER ORDEM,
/// qualquer um omitível (MDN). Classifica cada token: keyword de estilo → style;
/// largura (px/keyword) → width; senão tenta cor. Defaults CSS: style=none (se não
/// vier, a borda não aparece — o render checa `is_visible`), width=medium(3),
/// color=currentColor (aqui deixamos `border_color` como veio / herdado).
fn apply_border_shorthand(css: &mut ComputedStyle, val: &str) {
    // O curto escreve as DOZE longhands, não só as três uniformes: um lado
    // declarado antes dele é reposto (ver `borders::clear_sides`).
    crate::style::borders::clear_sides(css);
    for tok in val.split_whitespace() {
        if let Some(style) = BorderStyle::parse(tok) {
            set_if(&mut css.border_style, Some(style));
        } else if let Some(w) = parse_border_width_token(tok) {
            set_if(&mut css.border_width, Some(w));
        } else if let Some(d) = crate::style::borders::parse_width_dim(tok) {
            // `border: .3em solid` — a largura uniforme é um escalar em px e a
            // fonte deste nó ainda não está computada aqui (é herdada depois),
            // por isso o `em` vai para os QUATRO lados como dimensão e
            // `resolved_sides` resolve-o contra a fonte na hora (`claude-borda-em`:
            // 6px a 20px, não 4,8). O escalar fica com uma estimativa a 16px para
            // os consumidores antigos (`input.rs`, larguras de tabela). CORTE
            // dito: pelos lados, a pintura perde o `border-radius`.
            for lado in [crate::style::SideName::Top, crate::style::SideName::Right, crate::style::SideName::Bottom, crate::style::SideName::Left] {
                crate::style::borders::set_side_width_dim(css, lado, Some(d));
            }
            let rc = crate::style::ResolveCtx { parent_content_w: 0.0, node_font_size: 16.0, root_font_size: crate::style::root_font_size(), viewport_w: 0.0, viewport_h: 0.0 };
            set_if(&mut css.border_width, d.resolve(&rc));
        } else if let Some(c) = parse_color(tok) {
            set_if(&mut css.border_color, Some(c));
        }
        // token irreconhecível: ignora (robustez).
    }
    // `border: 2px red` sem estilo → o CSS exige style p/ aparecer; mas o shorthand
    // `border` RESETA o style para o default `solid`? Não — a spec diz que o
    // shorthand SETA todos os 3, e se o estilo for omitido vira `none`. Porém o uso
    // real quase sempre traz o estilo. Para fidelidade: se nenhum estilo veio no
    // shorthand, fica `none` (não pinta) — mas só se o width veio (senão é no-op).
    if css.border_style.is_none() && css.border_width.is_some() {
        set_if(&mut css.border_style, Some(BorderStyle::None));
    }
}

/// Largura de borda de um token: `thin`/`medium`/`thick` ou um comprimento px.
fn parse_border_width_token(tok: &str) -> Option<f32> {
    match tok.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => parse_px(tok),
    }
}

/// `font-weight`: `bold`/`bolder` ou peso numérico ≥ 600 → negrito.
fn is_bold(v: &str) -> bool {
    let v = v.trim();
    if v.eq_ignore_ascii_case("bold") || v.eq_ignore_ascii_case("bolder") {
        return true;
    }
    v.parse::<u32>().map(|w| w >= 600).unwrap_or(false)
}

/// `font-family: A, B, C` → a LISTA inteira, serializada como o Blink a
/// devolve em `getComputedStyle` (`FamiliaQueNaoExiste, monospace`; um nome
/// com espaços fica entre aspas duplas). Guardava-se só a primeira família —
/// e sem a lista não havia fallback para consultar: um `font-family:
/// Inexistente, monospace` media como proporcional (lote T,
/// `claude-font-unidades-ch-ex`).
fn parse_font_family(v: &str) -> Option<String> {
    let familias: Vec<String> = v
        .split(',')
        .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .filter(|f| !f.is_empty())
        .map(|f| match f.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            true => f.to_string(),
            false => format!("\"{f}\""),
        })
        .collect();
    (!familias.is_empty()).then(|| familias.join(", "))
}

/// `true` se a lista de famílias resolve numa fonte MONOESPAÇADA (o backend
/// usa para escolher o atlas mono). Percorre a lista como o browser: a
/// primeira família CONHECIDA decide — um nome que este motor não reconhece
/// é tratado como indisponível e salta-se para a seguinte, que é o que o
/// fallback do CSS faz com uma fonte que não está instalada. "Conhecida" é
/// uma keyword genérica ou um nome da lista curta abaixo; a alternativa —
/// decidir pela primeira família, conhecida ou não — era o que fazia
/// `Inexistente, monospace` medir como proporcional.
pub fn is_mono_family(name: &str) -> bool {
    for f in name.split(',') {
        let n = f.trim().trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase();
        if n.contains("mono") || n.contains("courier") || n.contains("consol") || n == "menlo" {
            return true;
        }
        let proporcional_conhecida = matches!(
            n.as_str(),
            "serif" | "sans-serif" | "cursive" | "fantasy" | "system-ui" | "ui-sans-serif"
                | "ui-serif" | "-apple-system" | "blinkmacsystemfont"
        ) || ["arial", "helvetica", "segoe", "roboto", "verdana", "georgia", "times",
            "inter", "tahoma", "trebuchet", "open sans", "lato", "noto", "ubuntu", "cambria",
            "calibri"]
            .iter()
            .any(|k| n.contains(k));
        if proporcional_conhecida {
            return false;
        }
    }
    false
}

/// `font: [style] [weight] size[/line-height] family` (shorthand). Parseia os
/// tokens posicionais: o `size` é o 1º comprimento; `/line-height` segue o size; o
/// resto antes do size são style/weight; o resto depois do size é a família.
/// ⚠️ CORTE: a spec diz que o shorthand RESETA as longhands omitidas ao valor
/// inicial (font sem `italic` zera o italic). Aqui só SETAMOS o que vem (não
/// resetamos o omitido) — `font-weight:bold; font:16px X` mantém o bold. E o size em
/// `em/rem/%` não resolve (parse_px só px), igual à longhand font-size.
fn apply_font_shorthand(css: &mut ComputedStyle, val: &str) {
    // separa o `size/line-height` (tem `/`) do resto.
    let tokens: Vec<&str> = val.split_whitespace().collect();
    let mut size_idx = None;
    for (i, t) in tokens.iter().enumerate() {
        // o token de size é o 1º que começa com dígito (ex: 16px, 1.2em, 16px/1.5).
        if t.chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            size_idx = Some(i);
            break;
        }
    }
    let Some(si) = size_idx else { return };
    // antes do size: style/weight.
    for t in &tokens[..si] {
        if t.eq_ignore_ascii_case("italic") || t.eq_ignore_ascii_case("oblique") {
            set_if(&mut css.italic, Some(true));
        } else if is_bold(t) {
            set_if(&mut css.bold, Some(true));
        }
    }
    // o size (e line-height opcional após `/`).
    let size_tok = tokens[si];
    let (sz, lh) = match size_tok.split_once('/') {
        Some((s, l)) => (s, Some(l)),
        None => (size_tok, None),
    };
    // px direto; se for relativo (em/rem/%), parse_px falha e fica None (herda) —
    // mesma limitação da longhand font-size (documentada).
    set_if(&mut css.font_size, parse_dimension(sz));
    if let Some(l) = lh {
        set_if(&mut css.line_height, LineHeight::parse(l));
    }
    // depois do size: a família.
    if si + 1 < tokens.len() {
        set_if(&mut css.font_family, parse_font_family(&tokens[si + 1..].join(" ")));
    }
}
