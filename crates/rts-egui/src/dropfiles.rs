//! Estado de arquivos soltos/pairando sobre uma janela (drag-and-drop do SO,
//! `WindowEvent::HoveredFile`/`HoveredFileCancelled`/`DroppedFile`).
//!
//! Puro Rust — nenhuma dependência de winit/egui — para ser testável sem abrir
//! janela (o mesmo motivo de `rts-input` ser um trait neutro: a lógica de
//! "quantos, quais, zera no próximo quadro" não precisa do SO para existir).
//! `app::Pumper` (winit) e `frame::begin_frame` (o relógio do quadro) são os
//! únicos lugares que tocam isto a partir de um evento real; `render_backend`
//! (`InputSource`) só LÊ.
//!
//! # O quadro dos arquivos soltos
//!
//! Como os eventos de tecla "pressed": o SO pode soltar vários arquivos numa
//! rajada de `DroppedFile` entre dois `pump()`, então eles se ACUMULAM até o
//! próximo `beginFrame`, que faz o SNAPSHOT (o que `droppedCount`/`droppedPath`
//! devolvem neste quadro) e ZERA o acumulador — o quadro seguinte já vê zero,
//! sem exigir que o TS "consuma" o evento. Os arquivos pairando (`hoveredFiles`)
//! não seguem esse padrão: são um estado contínuo (a contagem AGORA), não um
//! pulso de um quadro.
//!
//! # Posição
//!
//! Nem `HoveredFile` nem `DroppedFile` trazem a posição do cursor no winit, e o
//! SO não manda `CursorMoved` durante o arrasto. Por isso a posição é
//! responsabilidade de quem CHAMA estes métodos (`app`/`frame`, que sabem
//! consultar o SO — `GetCursorPos`+`ScreenToClient` no Windows, a última posição
//! conhecida do cursor nas demais plataformas) e não deste módulo, que só
//! guarda o que recebe.

/// Estado de drag-and-drop de UMA janela. Reaproveita os `Vec` entre quadros
/// (swap + `clear`, nunca um `Vec` novo) — zero alocação por quadro quando nada
/// foi solto.
#[derive(Debug)]
pub struct DropState {
    /// Quantos arquivos estão pairando AGORA (incrementa por `HoveredFile`,
    /// zera em `HoveredFileCancelled` ou ao soltar). Estado contínuo, não um
    /// pulso de quadro.
    hovered_count: usize,
    /// Posição do cursor (pontos lógicos) enquanto pairando; `(-1, -1)` sem
    /// nenhum arquivo pairando.
    hovered_pos: (f32, f32),
    /// Caminhos acumulados desde o último `snapshot_frame` (a rajada de
    /// `DroppedFile` do quadro corrente, ainda não publicada).
    dropped_accum: Vec<String>,
    /// Posição do cursor no momento da soltura mais recente ainda não
    /// publicada (todos os arquivos de uma rajada chegam praticamente na
    /// mesma posição; a última soltura da rajada é a que fica).
    dropped_pos_accum: (f32, f32),
    /// O snapshot deste quadro — o que `dropped_count`/`dropped_path` leem.
    dropped_frame: Vec<String>,
    /// A posição do snapshot deste quadro.
    dropped_pos_frame: (f32, f32),
}

/// Sem fonte/sem soltura/sem pairamento: `-1` é distinguível de qualquer
/// posição real (mesmo padrão de `input.mouseX`).
const NO_POS: (f32, f32) = (-1.0, -1.0);

impl DropState {
    pub fn new() -> Self {
        Self {
            hovered_count: 0,
            hovered_pos: NO_POS,
            dropped_accum: Vec::new(),
            dropped_pos_accum: NO_POS,
            dropped_frame: Vec::new(),
            dropped_pos_frame: NO_POS,
        }
    }

    /// `WindowEvent::HoveredFile` — mais um arquivo da seleção entrou na área
    /// da janela. `pos` é a posição do cursor já consultada pelo chamador.
    pub fn hovered_file(&mut self, pos: (f32, f32)) {
        self.hovered_count += 1;
        self.hovered_pos = pos;
    }

    /// `WindowEvent::HoveredFileCancelled` — o arrasto saiu da janela (ou foi
    /// cancelado) sem soltar.
    pub fn hovered_cancelled(&mut self) {
        self.hovered_count = 0;
        self.hovered_pos = NO_POS;
    }

    /// Chamado A CADA QUADRO (não só em evento): o SO não manda `CursorMoved`
    /// durante o arrasto, então a posição de hover só anda se alguém a
    /// consultar de novo aqui. No-op sem nada pairando.
    pub fn refresh_hover_pos(&mut self, pos: (f32, f32)) {
        if self.hovered_count > 0 {
            self.hovered_pos = pos;
        }
    }

    /// `WindowEvent::DroppedFile` — soltou um arquivo. `pos` é a posição do
    /// cursor no momento da soltura, já consultada pelo chamador.
    pub fn dropped_file(&mut self, path: String, pos: (f32, f32)) {
        // A soltura encerra o pairamento (mesmo comportamento do egui-winit:
        // um `DroppedFile` zera os `hovered_files` pendentes).
        self.hovered_count = 0;
        self.hovered_pos = NO_POS;
        self.dropped_accum.push(path);
        self.dropped_pos_accum = pos;
    }

    /// Publica a rajada acumulada como o snapshot DESTE quadro e zera o
    /// acumulador para o próximo — a "zeragem no quadro seguinte" do design.
    /// `swap` + `clear` em vez de um `Vec` novo: zero alocação quando não há
    /// nada para publicar (o caminho comum, todo quadro sem arrasto).
    pub fn snapshot_frame(&mut self) {
        std::mem::swap(&mut self.dropped_frame, &mut self.dropped_accum);
        self.dropped_accum.clear();
        if !self.dropped_frame.is_empty() {
            self.dropped_pos_frame = self.dropped_pos_accum;
        }
    }

    /// `input.droppedCount(win)`.
    pub fn dropped_count(&self) -> usize {
        self.dropped_frame.len()
    }

    /// `input.droppedPath(win, i)` — vazio fora da faixa.
    pub fn dropped_path(&self, index: usize) -> &str {
        self.dropped_frame.get(index).map(String::as_str).unwrap_or("")
    }

    /// `input.droppedX/Y(win)` — `(-1, -1)` sem soltura neste quadro.
    pub fn dropped_pos(&self) -> (f32, f32) {
        if self.dropped_frame.is_empty() {
            NO_POS
        } else {
            self.dropped_pos_frame
        }
    }

    /// `input.hoveredFiles(win)`.
    pub fn hovered_files(&self) -> usize {
        self.hovered_count
    }

    /// `input.hoveredX/Y(win)` — `(-1, -1)` sem nada pairando.
    pub fn hovered_pos(&self) -> (f32, f32) {
        if self.hovered_count == 0 {
            NO_POS
        } else {
            self.hovered_pos
        }
    }
}

impl Default for DropState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nada_solto_e_nada_pairando_por_padrao() {
        let s = DropState::new();
        assert_eq!(s.dropped_count(), 0);
        assert_eq!(s.dropped_path(0), "");
        assert_eq!(s.dropped_pos(), NO_POS);
        assert_eq!(s.hovered_files(), 0);
        assert_eq!(s.hovered_pos(), NO_POS);
    }

    #[test]
    fn pairar_conta_por_arquivo_e_atualiza_posicao() {
        let mut s = DropState::new();
        s.hovered_file((10.0, 20.0));
        s.hovered_file((11.0, 21.0)); // 2º arquivo da mesma seleção
        assert_eq!(s.hovered_files(), 2);
        assert_eq!(s.hovered_pos(), (11.0, 21.0));
    }

    #[test]
    fn refresh_hover_pos_so_anda_enquanto_pairando() {
        let mut s = DropState::new();
        s.refresh_hover_pos((5.0, 5.0)); // sem nada pairando: no-op
        assert_eq!(s.hovered_pos(), NO_POS);

        s.hovered_file((1.0, 1.0));
        s.refresh_hover_pos((2.0, 2.0)); // o SO não manda CursorMoved; isto é o substituto
        assert_eq!(s.hovered_pos(), (2.0, 2.0));
    }

    #[test]
    fn cancelar_zera_o_pairamento() {
        let mut s = DropState::new();
        s.hovered_file((1.0, 1.0));
        s.hovered_cancelled();
        assert_eq!(s.hovered_files(), 0);
        assert_eq!(s.hovered_pos(), NO_POS);
    }

    #[test]
    fn soltar_acumula_e_so_aparece_apos_o_snapshot() {
        let mut s = DropState::new();
        s.hovered_file((1.0, 1.0));
        s.dropped_file("C:/a.wav".to_string(), (30.0, 40.0));
        s.dropped_file("C:/b.wav".to_string(), (31.0, 41.0));
        // ainda não publicado: o quadro corrente não viu snapshot_frame.
        assert_eq!(s.dropped_count(), 0);
        // soltar encerra o pairamento imediatamente (como o egui-winit faz).
        assert_eq!(s.hovered_files(), 0);

        s.snapshot_frame();
        assert_eq!(s.dropped_count(), 2);
        assert_eq!(s.dropped_path(0), "C:/a.wav");
        assert_eq!(s.dropped_path(1), "C:/b.wav");
        assert_eq!(s.dropped_path(2), ""); // fora da faixa
        assert_eq!(s.dropped_pos(), (31.0, 41.0));
    }

    #[test]
    fn zera_no_quadro_seguinte_sem_nova_soltura() {
        let mut s = DropState::new();
        s.dropped_file("C:/a.wav".to_string(), (1.0, 2.0));
        s.snapshot_frame(); // quadro N: publica
        assert_eq!(s.dropped_count(), 1);

        s.snapshot_frame(); // quadro N+1: nada novo desde o último snapshot
        assert_eq!(s.dropped_count(), 0);
        assert_eq!(s.dropped_path(0), "");
        assert_eq!(s.dropped_pos(), NO_POS);
    }

    #[test]
    fn snapshot_sem_nada_novo_nao_apaga_a_posicao_se_vazio_mesmo() {
        // Regressão de detalhe: um snapshot vazio não deve "ressuscitar" uma
        // posição antiga junto de uma lista vazia — dropped_pos() já cobre
        // isso lendo dropped_frame.is_empty(), não dropped_pos_frame direto.
        let mut s = DropState::new();
        s.dropped_file("C:/a.wav".to_string(), (7.0, 8.0));
        s.snapshot_frame();
        s.snapshot_frame();
        s.snapshot_frame();
        assert_eq!(s.dropped_pos(), NO_POS);
    }
}
