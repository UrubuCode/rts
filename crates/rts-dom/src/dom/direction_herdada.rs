//! O `direction` a usar como FALLBACK para resolver uma `margin-inline-*`/
//! `padding-inline-*`/`border-inline-*` pendente (`style::logical`,
//! `dom::cascade::computed_style_idx_inner`) — normalmente o herdado do pai,
//! com um corte: não quando o pai é uma LINHA de flex (`row`/`row-reverse`).
//!
//! ## O corte, dito por extenso
//!
//! Uma linha de flex sob `direction:rtl` devia, pela spec (Flexbox
//! "Directions" + Writing Modes), trocar main-start para a borda DIREITA —
//! ou seja, inverter também a ORDEM dos itens, do mesmo jeito que
//! `row-reverse` já faz. Esse lado do trabalho não está feito (o motor não
//! reordena uma linha por `direction`, só por `flex-direction`) — e sem ele,
//! ter a MARGEM certa mas a ORDEM errada é uma combinação pior do que ter as
//! duas erradas: o vão sai no lado físico oposto ao dos itens.
//!
//! Achado no retrabalho do lote `flex-reverse-order`: os WPT `gap-003-rtl`/
//! `gap-006-rtl` (linhas RTL com `margin-inline-end` a simular um `gap`)
//! passavam por essa MESMA coincidência que `gap-007-rtl` explorava do lado
//! da coluna — margem errada, ordem errada, os dois erros a bater um no
//! outro. Corrigir só a margem (sem corrigir a ordem) piora esses dois: a
//! tentativa de corrigir TAMBÉM a ordem (`bloco.rs`, revertida) esbarrou
//! numa causa maior — reverter a lista de itens ANTES do agrupamento em
//! linhas (`flex.rs`, o mesmo código que `row-reverse` já usa) muda QUAIS
//! itens partilham linha quando há `flex-wrap` e margens por item
//! distinguem-nos (`:nth-child`) — a mesma classe de bug que `coluna_wrap.rs`
//! já tinha resolvido para colunas (agrupar na ordem do documento, só
//! espelhar a POSIÇÃO depois), aqui ainda por fazer para linhas.
//!
//! Por isso esta função devolve `None` (cai no inicial, `ltr`) quando o pai
//! é uma linha — o MESMO resultado que existia antes deste lote para essa
//! combinação — e só deixa o `direction` herdado passar para colunas, blocos
//! e tudo o resto, onde a ordem dos itens já é a correta.
use crate::style::{ComputedStyle, Direction, DisplayKind};

/// `direction` do PAI para o fallback de `style::logical`, ou `None` se o
/// pai é uma linha de flex (ver o cabeçalho).
pub(in crate::dom) fn para_logicas(pai: Option<&ComputedStyle>) -> Option<Direction> {
    let pai = pai?;
    let e_linha_de_flex = matches!(
        pai.effective_display(),
        Some(DisplayKind::Flex | DisplayKind::FlexWrap | DisplayKind::InlineFlex | DisplayKind::InlineFlexWrap)
    ) && !pai.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    if e_linha_de_flex {
        return None;
    }
    pai.direction
}
