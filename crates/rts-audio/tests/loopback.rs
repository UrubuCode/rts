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

/// Faz o polling de `escuta::escuta_ler` até ela terminar (ou 5 s, o que
/// nunca deveria acontecer com uma escuta de 300 ms — o timeout é só pra não
/// travar o processo de teste se algo der muito errado).
fn aguardar_escuta_ler() -> escuta::EscutaContinua {
    for _ in 0..500 {
        if let Some(r) = escuta::escuta_ler() { return r; }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("escuta_ler não terminou depois de 5 s de polling");
}

/// A THREAD CHAMADORA nunca bloqueia aqui: `escuta_iniciar` devolve na hora, e
/// o polling poderia ser feito por quadro do jogo em vez do laço de espera
/// acima (que só existe porque este teste precisa do resultado final antes
/// de terminar). Toca 997 Hz BEM baixinho (amplitude 0,05, pra o teste não
/// depender de outro som mais alto na máquina) e espera o detector de
/// Goertzel achar energia clara nessa frequência.
#[test]
#[ignore]
fn escuta_continua_detecta_997hz_tocando_baixinho() {
    let saida = device::abrir(&Pedido { taxa: 0, canais: 0, nulo: false })
        .expect("dispositivo de saída real (rode fora de CI, com placa de som)");
    let taxa = saida.taxa as f64;
    let canais = saida.canais.max(1) as usize;
    let comp = saida.comp.clone();
    let parar = Arc::new(AtomicBool::new(false));
    let p = parar.clone();

    let tocando = thread::spawn(move || {
        let passo = 2.0 * std::f64::consts::PI * 997.0 / taxa;
        let mut fase = 0.0f64;
        let mut bloco = vec![0.0f32; 480 * canais];
        while !p.load(Ordering::Relaxed) {
            for quadro in 0..480 {
                let v = (fase.sin() * 0.05) as f32;
                for c in 0..canais { bloco[quadro * canais + c] = v; }
                fase += passo;
            }
            comp.anel.escrever_quadros(&bloco, canais);
            thread::sleep(Duration::from_millis(5));
        }
    });

    thread::sleep(Duration::from_millis(50));
    assert!(escuta::escuta_iniciar(300, 997.0), "deveria conseguir iniciar a escuta contínua");
    let r = aguardar_escuta_ler();

    parar.store(true, Ordering::Relaxed);
    tocando.join().expect("thread do tom não deveria entrar em pânico");
    drop(saida);

    println!("escuta contínua com 997 Hz baixinho (amplitude 0,05): {r:?}");
    assert!(r.energia_freq > 0.01, "energiaFreq baixa demais pro tom de 997 Hz: {r:?}");
}

/// SEM tocar 997 Hz — mas NÃO afirma "nada tocando": pode haver outro som na
/// máquina (o próprio SO, outra aplicação). O que o detector precisa garantir
/// é não confundir esse outro som com o alvo: `energiaFreq` continua baixa.
#[test]
#[ignore]
fn escuta_continua_sem_997hz_da_energia_quase_zero() {
    assert!(escuta::escuta_iniciar(300, 997.0), "deveria conseguir iniciar a escuta contínua");
    let r = aguardar_escuta_ler();
    println!("escuta contínua sem 997 Hz tocando (pode haver outro som na máquina): {r:?}");
    assert!(r.energia_freq < 0.02, "energiaFreq alta demais sem o tom de 997 Hz: {r:?}");
}
