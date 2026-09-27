//! Testes de integração da escuta por loopback com um dispositivo de som
//! REAL (não o nulo). Ignorados por padrão — exigem uma placa de som de
//! verdade e travam a máquina de CI a menos que peçam por eles:
//!
//! ```text
//! cargo test -p rts-audio -- --ignored
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use rts_audio::device::{self, Pedido};
use rts_audio::escuta;

/// Toca um tom de 440 Hz pela saída real enquanto `escuta::escutar` roda em
/// paralelo, e espera achar som: `!silencio` e `rms > 0.001`.
#[test]
#[ignore]
fn escuta_ve_um_tom_de_440hz_tocando() {
    let saida = device::abrir(&Pedido { taxa: 0, canais: 0, nulo: false })
        .expect("dispositivo de saída real (rode fora de CI, com placa de som)");
    let taxa = saida.taxa as f64;
    let canais = saida.canais.max(1) as usize;
    let comp = saida.comp.clone();
    let parar = Arc::new(AtomicBool::new(false));
    let p = parar.clone();

    let tocando = thread::spawn(move || {
        let passo = 2.0 * std::f64::consts::PI * 440.0 / taxa;
        let mut fase = 0.0f64;
        let mut bloco = vec![0.0f32; 480 * canais];
        while !p.load(Ordering::Relaxed) {
            for quadro in 0..480 {
                let v = (fase.sin() * 0.5) as f32;
                for c in 0..canais { bloco[quadro * canais + c] = v; }
                fase += passo;
            }
            comp.anel.escrever_quadros(&bloco, canais);
            thread::sleep(Duration::from_millis(5));
        }
    });

    // Dá tempo do tom começar a soar antes de abrir o loopback.
    thread::sleep(Duration::from_millis(50));
    let r = escuta::escutar(300);

    parar.store(true, Ordering::Relaxed);
    tocando.join().expect("thread do tom não deveria entrar em pânico");
    drop(saida);

    let r = r.expect("loopback deveria funcionar com um dispositivo de saída real");
    println!("escuta com tom de 440 Hz: {r:?}");
    assert!(!r.silencio, "esperava som, veio silêncio: {r:?}");
    assert!(r.rms > 0.001, "rms baixo demais: {r:?}");
}

/// Escuta sem tocar nada pelo crate. NÃO afirma silêncio — o usuário pode ter
/// outro som tocando na máquina — só imprime para o relatório/inspeção.
#[test]
#[ignore]
fn escuta_sem_tocar_nada_so_imprime() {
    let r = escuta::escutar(300)
        .expect("loopback deveria funcionar com um dispositivo de saída real");
    println!("escuta sem tocar nada (pode haver outro som na máquina): {r:?}");
}
