//! Scroll — o offset da PÁGINA e de cada região `overflow:auto`/`scroll`,
//! vivendo em `Dom` em vez de só no backend (finding 3 da auditoria
//! estrutural, `docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/
//! 01-pipeline-e-fronteiras.md`). Mesmo padrão de `hovered`/`focused_input`
//! (`dom/mod.rs`): estado de DOCUMENTO, não de backend — o egui lê para
//! pintar e traduzir o hit-test, e escreve só em resposta a input.
//!
//! ## A decisão: offset FORA da chave de cache, aplicado como translação
//!
//! O layout (`layout/bloco.rs`) emite cada `BeginClip` com o offset LIDO daqui
//! (`scroll_of`) — mas isso é só o valor "como estava quando este fragmento
//! foi montado"; nem a pintura nem uma consulta de geometria confiam nele.
//! Os dois voltam a perguntar ao `Dom` o valor VIVO no momento em que
//! precisam (a pintura, em `paint_list`; a geometria, aqui em
//! [`Dom::bounding_rect_scrolled`]). A alternativa era pôr o offset na chave
//! do cache de fragmento/`DisplayList` (`FragmentKey`/`DisplayKey`,
//! `dom/mod.rs`): cada tick da roda do rato seria um cache MISS do
//! documento inteiro (a `DisplayList` tem UM slot, chaveado por revisão+
//! viewport+medidor — não por região), e mesmo escopado ao `FragmentKey` da
//! região rolada isso obrigaria a REFAZER o box model e os filhos a cada
//! tick, para no fim escrever exatamente os mesmos números — só os dois
//! `f32` do `BeginClip` mudam com o scroll, o resto do fragmento é idêntico.
//! Reler o `Dom` custa um `Cell::get`/lookup num `HashMap` pequeno; refazer o
//! layout custa a subárvore inteira. Por isso scroll NUNCA invalida cache
//! nenhum — nem `touch()` nem `touch_render_only()` são chamados daqui.
//!
//! ## Por que dois pares get/set (um que clampa sozinho, um que recebe o teto)
//!
//! `set_scroll`/`set_page_scroll` fazem o PRÓPRIO layout headless (medidor
//! aproximado) para descobrir `scrollHeight`/`scrollWidth` e clampar — o
//! mesmo custo que [`Dom::bounding_component`](super::Dom::bounding_component)
//! já paga por chamada, aceitável porque quem chama por aqui é o bridge
//! (`el.scrollTop = x`) ou um teste, nunca por frame.
//!
//! `set_scroll_extent`/`set_page_scroll_extent` recebem os limites de quem já
//! os tem: o backend (`rts-egui`) já correu `layout_cached` este frame com o
//! medidor REAL para pintar, e pedir aqui um SEGUNDO layout — headless,
//! aproximado — só para clampar pagaria o documento inteiro a cada tick da
//! roda do rato, com uma métrica diferente da que vai ser pintada. É o
//! caminho quente; os dois com sufixo `_extent` existem só por causa dele.

use super::*;

impl Dom {
    /// O offset de scroll de uma região `overflow:auto`/`scroll`, `(x, y)` em
    /// pontos de conteúdo. `(0.0, 0.0)` para um nó que nunca rolou ou que não
    /// resolve — o mesmo "sem estado ainda" que um browser responde para
    /// `scrollTop` de um elemento que nunca rolou.
    pub fn scroll_of(&self, id: NodeId) -> (f32, f32) {
        let Some(idx) = self.resolve(id) else {
            return (0.0, 0.0);
        };
        self.scroll_of_idx(idx)
    }

    /// A mesma leitura, por índice CRU — o layout (`layout/bloco.rs`) já
    /// trabalha em `NodeIdx`, e é o mesmo tipo que a `DisplayList` já expõe
    /// a quem pinta (`ScrollRegion::node_idx`, `DisplayItem::BeginClip::node`
    /// já cruzam para o `rts-egui` assim, sem passar por um `NodeId`
    /// versionado) — `pub` pela mesma razão: quem já recebe o índice cru de
    /// uma dessas duas estruturas não tem porque empacotar/resolver um
    /// `NodeId` só para ler um par de `f32`. A API pensada para o BRIDGE
    /// (que só tem o `NodeId` da ABI) é `scroll_of`.
    pub fn scroll_of_idx(&self, idx: NodeIdx) -> (f32, f32) {
        self.scroll_regioes.borrow().get(&idx).copied().unwrap_or((0.0, 0.0))
    }

    /// O offset de scroll da PÁGINA, `(x, y)`.
    pub fn page_scroll(&self) -> (f32, f32) {
        self.scroll.get()
    }

    /// `(scrollWidth, scrollHeight, clientWidth, clientHeight)` de um nó.
    ///
    /// Fora de uma região rolável, os dois pares respondem o MESMO valor — a
    /// caixa do próprio elemento — porque sem conteúdo maior que a caixa não
    /// há o que rolar, que é a resposta que um browser também dá. Roda um
    /// layout headless (mesma pergunta que `bounding_component`, mesmo
    /// custo): não é um caminho de frame.
    pub fn scroll_extent(&self, id: NodeId) -> (f32, f32, f32, f32) {
        let Some(idx) = self.resolve(id) else {
            return (0.0, 0.0, 0.0, 0.0);
        };
        let (vw, vh) = self.viewport.get();
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(self, &ctx);
        if let Some(region) = list.geometry().scroll_regions.iter().find(|r| r.node_idx == idx) {
            return (region.content_w, region.content_h, region.visible.w, region.visible.h);
        }
        match list.rect_of(idx) {
            Some(r) => (r.w, r.h, r.w, r.h),
            None => (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// `getBoundingClientRect` de `id` com o scroll aplicado — o que
    /// `bounding_component` (`dom/geometria.rs`) não faz, porque não conhece
    /// scroll (é exatamente o furo que esta fatia fecha). Sobe pelos
    /// ancestrais subtraindo o offset de cada região rolável que encontra no
    /// caminho, e por fim o da PÁGINA — a mesma soma que a pintura já faz em
    /// `paint_list` (`BeginClip` empilha `-offset`).
    ///
    /// Método PRÓPRIO e não uma mudança em `bounding_component`: aquela
    /// função está a mudar AO MESMO TEMPO no lote do medidor ativo
    /// (`PLAN.md` §4.B — troca o `ApproxMeasurer` fixo pelo medidor
    /// registado), e duas razões para mexer na mesma função ao mesmo tempo é
    /// o que a regra "um lote, um ficheiro" existe para evitar. Juntar os
    /// dois (scroll + medidor ativo) é trabalho de integração, não deste
    /// lote — fica dito no relatório do commit.
    pub fn bounding_rect_scrolled(&self, id: NodeId) -> Option<crate::layout::Rect> {
        let idx = self.resolve(id)?;
        let (vw, vh) = self.viewport.get();
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(self, &ctx);
        let geometry = list.geometry();
        let mut rect = *geometry.rects.get(&idx)?;
        let mut cur = self.nodes[idx].parent;
        while let Some(a) = cur {
            if geometry.scroll_regions.iter().any(|r| r.node_idx == a) {
                let (ox, oy) = self.scroll_of_idx(a);
                rect.x -= ox;
                rect.y -= oy;
            }
            cur = self.nodes[a].parent;
        }
        let (px, py) = self.scroll.get();
        rect.x -= px;
        rect.y -= py;
        Some(rect)
    }

    /// Escreve o offset de uma região, CLAMPADO ao conteúdo dela — corre o
    /// próprio layout (headless) para descobrir o teto. Ver a nota de topo do
    /// ficheiro sobre por que só este par (não o `_extent`) paga esse custo.
    /// Um `id` que não resolve, ou que não é uma região rolável, é um no-op
    /// silencioso (mesma regra do resto do bridge: um handle errado não
    /// aplica estado a um nó qualquer).
    pub fn set_scroll(&mut self, id: NodeId, x: f32, y: f32) {
        let Some(idx) = self.resolve(id) else { return };
        let (vw, vh) = self.viewport.get();
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(self, &ctx);
        let (max_x, max_y) = list
            .geometry()
            .scroll_regions
            .iter()
            .find(|r| r.node_idx == idx)
            .map(|r| ((r.content_w - r.visible.w).max(0.0), (r.content_h - r.visible.h).max(0.0)))
            .unwrap_or((0.0, 0.0));
        self.set_scroll_extent_idx(idx, x, y, max_x, max_y);
    }

    /// Escreve o offset de uma região com o teto que QUEM CHAMA já conhece
    /// (o backend, que correu `layout_cached` este frame). Ver a nota de topo
    /// do ficheiro — é o caminho quente (roda do rato/arrastar a barra, uma
    /// vez por frame), e não paga layout nenhum aqui.
    pub fn set_scroll_extent(&mut self, id: NodeId, x: f32, y: f32, max_x: f32, max_y: f32) {
        let Some(idx) = self.resolve(id) else { return };
        self.set_scroll_extent_idx(idx, x, y, max_x, max_y);
    }

    /// A mesma escrita, por índice CRU — o `rts-egui` já tem o `NodeIdx` de
    /// `ScrollRegion::node_idx` em mãos (a `DisplayList` já o expõe assim,
    /// ver `scroll_of_idx`) e é ele quem chama isto por frame; empacotar um
    /// `NodeId` só para desempacotar de volta não pagaria nada.
    pub fn set_scroll_extent_idx(&mut self, idx: NodeIdx, x: f32, y: f32, max_x: f32, max_y: f32) {
        let clamped = (x.clamp(0.0, max_x.max(0.0)), y.clamp(0.0, max_y.max(0.0)));
        let prev = self.scroll_regioes.borrow().get(&idx).copied().unwrap_or((0.0, 0.0));
        if prev == clamped {
            return;
        }
        self.scroll_regioes.borrow_mut().insert(idx, clamped);
        // "scroll" não borbulha na spec — o pump genérico (`pumpEventCallbacks`)
        // só sabe despachar COM bubbling; a divergência fica aqui documentada
        // em vez de escondida (nenhum teste desta fatia depende de não
        // borbulhar, e escrever um segundo caminho de dispatch só para isto
        // não paga o peso agora).
        self.push_raw_event(idx, "scroll");
    }

    /// Escreve o scroll da PÁGINA, clampado ao `content_height` do documento
    /// (corre o próprio layout headless — ver a nota de topo). O eixo X não é
    /// clampado ao TETO: este motor não mede overflow horizontal de página
    /// (nenhuma `ScrollRegion` cobre o documento inteiro, só containers
    /// internos), então só o mínimo (`>= 0`) é imposto — divergência
    /// declarada, não escondida atrás de um clamp que fingiria medir algo que
    /// não mede.
    pub fn set_page_scroll(&mut self, x: f32, y: f32) {
        let (vw, vh) = self.viewport.get();
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let content_h = crate::layout::layout_document(self, &ctx).content_height;
        self.write_page_scroll(x, y, (content_h - vh).max(0.0));
    }

    /// Mesma escrita, com o `content_height` que quem chama já tem (o
    /// backend, por frame — ver a nota de topo).
    pub fn set_page_scroll_extent(&mut self, x: f32, y: f32, max_y: f32) {
        self.write_page_scroll(x, y, max_y);
    }

    fn write_page_scroll(&mut self, x: f32, y: f32, max_y: f32) {
        let clamped = (x.max(0.0), y.clamp(0.0, max_y.max(0.0)));
        if self.scroll.get() == clamped {
            return;
        }
        self.scroll.set(clamped);
        // alvo do evento "scroll" da página: o mesmo que `WindowImpl.
        // addEventListener` já usa para os eventos de nível de janela
        // (`window.ts`, `root = document.querySelector("body")`) — sem isso
        // `window.addEventListener("scroll", cb)` nunca dispararia.
        let target = self
            .query_idx("body")
            .or_else(|| self.document_element().map(|id| id.idx as usize))
            .unwrap_or(self.root);
        self.push_raw_event(target, "scroll");
    }

    /// `el.scrollIntoView()` — mínimo: alinha o TOPO de `id` com o topo da
    /// região rolável mais próxima (subindo pelos ancestrais) ou, se nenhuma
    /// existir no caminho, com o topo da PÁGINA. Sem opções (`block`/
    /// `inline`, `behavior: smooth`) — é o que um browser faz sem argumento
    /// nenhum, e é só o que os cortes deste lote pedem.
    ///
    /// Não escala pra além do primeiro ancestral rolável: um browser rola
    /// TODOS os ancestrais no caminho até o alvo ficar visível; aqui só o
    /// mais próximo. Documentado, não escondido — o cenário que o corpus
    /// desta fatia cobre (`examples/claude-tarefas.ts`) não aninha regiões.
    pub fn scroll_into_view(&mut self, id: NodeId) {
        let Some(idx) = self.resolve(id) else { return };
        let (vw, vh) = self.viewport.get();
        let ctx = crate::layout::LayoutCtx {
            viewport_w: vw,
            viewport_h: vh,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(self, &ctx);
        let geometry = list.geometry();
        let Some(&target_rect) = geometry.rects.get(&idx) else { return };
        let mut cur = self.nodes[idx].parent;
        while let Some(a) = cur {
            if let Some(region) = geometry.scroll_regions.iter().find(|r| r.node_idx == a) {
                let local_x = target_rect.x - region.visible.x;
                let local_y = target_rect.y - region.visible.y;
                let max_x = (region.content_w - region.visible.w).max(0.0);
                let max_y = (region.content_h - region.visible.h).max(0.0);
                self.set_scroll_extent_idx(a, local_x, local_y, max_x, max_y);
                return;
            }
            cur = self.nodes[a].parent;
        }
        // nenhuma região no caminho: quem rola é a PÁGINA.
        let max_y = (list.content_height - vh).max(0.0);
        self.write_page_scroll(target_rect.x, target_rect.y, max_y);
    }
}
