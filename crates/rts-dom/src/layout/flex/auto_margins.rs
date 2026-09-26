//! Margens `auto` no EIXO TRANSVERSAL de um item flex (Flexbox §8.1).
//!
//! No eixo principal `flex.rs` e `coluna.rs` já repartiam o espaço livre pelas
//! margens `auto` (é o `mx-auto`). No transversal nada as lia: `margin: auto`
//! centrava na horizontal e ficava colado ao topo. O `auto-margins-001` do WPT
//! passava VAZIO até as bordas em `em` pintarem (lote borda-em); com bordas
//! reais as três circunferências saíam encostadas ao topo. A régua é
//! `claude-flex-margens-auto-transversal` (Edge 152).
//!
//! A regra da spec: as margens `auto` absorvem o espaço livre transversal e
//! VENCEM o `align-self` — `margin-top: auto` sozinho empurra o item para o
//! fundo mesmo com `align-items: flex-start`, e um item com margens `auto`
//! nunca estica. Com espaço livre negativo as margens valem 0 (o item
//! transborda pelo lado de fim, como se fosse `flex-start`).
//!
//! Só a LINHA passa por aqui: em coluna o eixo transversal é o horizontal e o
//! layout de bloco do item já resolve `margin-left/right: auto` sozinho
//! (centrar, encostar à direita) — fazê-lo aqui também centrava duas vezes
//! (c1 saía em 150 em vez de 75). Corte dito: um item de coluna com
//! `width: auto` e margens `auto` fica esticado em vez de fit-content.

/// O deslocamento transversal do item quando alguma margem é `auto`; `None`
/// quando nenhuma é — o chamador cai então no `align-items`/stretch.
pub(super) fn off_cross(auto_inicio: bool, auto_fim: bool, linha: f32, item: f32) -> Option<f32> {
    if !auto_inicio && !auto_fim {
        return None;
    }
    let livre = (linha - item).max(0.0);
    Some(match (auto_inicio, auto_fim) {
        (true, true) => livre / 2.0,
        (true, false) => livre,
        _ => 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::off_cross;

    #[test]
    fn as_duas_auto_centram_e_uma_so_empurra_para_o_seu_lado() {
        assert_eq!(off_cross(false, false, 100.0, 30.0), None, "sem auto manda o align");
        assert_eq!(off_cross(true, true, 100.0, 30.0), Some(35.0));
        assert_eq!(off_cross(true, false, 100.0, 30.0), Some(70.0), "margin-top: auto = fundo");
        assert_eq!(off_cross(false, true, 100.0, 30.0), Some(0.0), "margin-bottom: auto = topo");
    }

    #[test]
    fn sem_espaco_livre_as_margens_auto_valem_zero() {
        assert_eq!(off_cross(true, true, 20.0, 30.0), Some(0.0));
    }
}
