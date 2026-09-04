//! `getBoundingClientRect` — a geometria pedida de fora, singular e em lote.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// `getBoundingClientRect(el)[componente]` — 0=x, 1=y, 2=largura, 3=altura.
    ///
    /// Mede com o medidor ACTIVO da thread (`layout::medidor_ativo`): quando
    /// há uma janela real aberta, é o mesmo `TextMeasurer` de fontes reais que
    /// `render_dom` usa para pintar — a mesma geometria que o JS lê é a que sai
    /// na tela. Sem janela (headless), `with_active` cai sozinho no medidor
    /// APROXIMADO: não há fonte real, e devolver zero seria pior do que a
    /// aproximação que o layout headless já usa em todo o resto.
    ///
    /// Esta função e `render_dom` ainda fazem DUAS passadas de layout — uma
    /// árvore de fragmentos única e partilhada é o redesenho maior que o
    /// finding que isto fecha aponta como fora de âmbito aqui —, mas agora as
    /// duas usam o mesmo medidor, o que é o que fazia as duas respostas
    /// divergirem.
    pub fn bounding_component(&self, id: NodeId, which: i64) -> f32 {
        let Some(idx) = self.resolve(id) else {
            return 0.0;
        };
        let (vw, vh) = self.viewport.get();
        crate::layout::medidor_ativo::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx {
                viewport_w: vw,
                viewport_h: vh,
                measurer,
            };
            let Some(rect) = crate::layout::bounding_rect(self, idx, &ctx) else {
                return 0.0;
            };
            match which {
                0 => rect.x,
                1 => rect.y,
                2 => rect.w,
                _ => rect.h,
            }
        })
    }

    /// As quatro componentes da caixa de MUITOS nós de uma vez, na ordem
    /// `x, y, w, h` por nó pedido.
    ///
    /// Existe porque `bounding_component` faz um `layout_document` INTEIRO por
    /// chamada, e isso é linear no documento: medido a 13,7 ms por chamada na
    /// Wikipédia (16 813 elementos). O extrator de paridade pede quatro
    /// componentes por elemento, ou seja ~67 mil layouts completos do mesmo
    /// documento imutável — os 9m21s que a extração de paridade custava eram
    /// isto, e não o layout, que precisa de correr uma vez. Com esta: 9,7s, e o
    /// dump a sair byte a byte igual ao de antes.
    ///
    /// **Não é um cache, de propósito.** O layout é feito AQUI DENTRO, nesta
    /// chamada, e não sobrevive a ela: não há estado entre chamadas que possa
    /// ficar velho depois de uma mutação. Um `DisplayList` guardado no `Dom`
    /// seria mais rápido para todos os consumidores e traria a pergunta de
    /// invalidação para um sítio que hoje não a tem — e uma geometria que não
    /// reflete o DOM não é uma medição mais rápida, é outra medição.
    ///
    /// Um id que não resolve responde `0.0` nas quatro, que é exatamente o que
    /// `bounding_component` responde no mesmo caso.
    pub fn bounding_components_many(&self, ids: &[NodeId]) -> Vec<f32> {
        let (vw, vh) = self.viewport.get();
        crate::layout::medidor_ativo::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx {
                viewport_w: vw,
                viewport_h: vh,
                measurer,
            };
            let list = crate::layout::layout_document(self, &ctx);
            let mut out = Vec::with_capacity(ids.len() * 4);
            for &id in ids {
                match self.resolve(id).and_then(|idx| list.rect_of(idx)) {
                    Some(r) => out.extend_from_slice(&[r.x, r.y, r.w, r.h]),
                    None => out.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]),
                }
            }
            out
        })
    }

    /// Igual a [`crate::layout::DisplayList::hit_test`], mas `pointer-events:
    /// none` fica TRANSPARENTE ao clique — a espessura de `hit_order` sob o
    /// nó de topo é revisitada até achar um nó cujo computado não seja
    /// `none` (herda, como a spec pede, e o `ComputedStyle` de cada nó já
    /// reflete a herança — não há necessidade de subir a árvore aqui).
    ///
    /// Vive no `Dom` e não em `DisplayList::hit_test` porque só o `Dom` tem a
    /// cascade; a lista de exibição só tem retângulos e índices. Quem decide
    /// clique (`rts-egui`) chamava `DisplayList::hit_test` direto — passar a
    /// chamar este em vez daquele é a mudança mínima que fecha o gap.
    pub fn hit_test_clickable(&self, list: &crate::layout::DisplayList, x: f32, y: f32) -> Option<NodeIdx> {
        let g = list.geometry();
        g.hit_order.iter().rev().copied().find(|&idx| {
            let dentro = g
                .rects
                .get(&idx)
                .is_some_and(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h);
            dentro
                && !matches!(
                    self.computed_style_idx(idx).and_then(|s| s.pointer_events),
                    Some(crate::style::vocab::PointerEvents::None)
                )
        })
    }
}
