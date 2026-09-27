//! O anel entre o programa (produtor) e a thread de áudio (consumidor).
//!
//! # Por que `AtomicU32` e não `UnsafeCell<f32>`
//!
//! Um produtor e um consumidor em threads diferentes sobre a mesma memória.
//! Guardar os bits do `f32` em `AtomicU32` com `Relaxed` compila no x86 e no ARM
//! para o mesmo `mov` de um `f32` comum, e a ordem entre os índices é o par
//! `Release`/`Acquire` de `escrita`/`leitura`. Sai sem uma linha de `unsafe`.
//!
//! Os índices são contadores monotônicos de AMOSTRAS (não de quadros) em `u64`:
//! a diferença é o que está enfileirado, e a máscara dá a posição no vetor. A
//! 192 kHz × 8 canais, `u64` dá a volta em 380 mil anos.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Anel SPSC de `f32` intercalados. Um produtor (o programa) e um consumidor
/// (a thread do dispositivo), nunca mais que isso.
pub struct Anel {
    dados: Box<[AtomicU32]>,
    mascara: u64,
    escrita: AtomicU64,
    leitura: AtomicU64,
}

impl Anel {
    /// Um anel com pelo menos `minimo` amostras, arredondado para potência de 2
    /// (mínimo 2).
    pub fn new(minimo: usize) -> Anel {
        let cap = minimo.max(2).next_power_of_two();
        let dados: Box<[AtomicU32]> = (0..cap).map(|_| AtomicU32::new(0)).collect();
        Anel { dados, mascara: (cap - 1) as u64, escrita: AtomicU64::new(0), leitura: AtomicU64::new(0) }
    }

    /// Quantas amostras cabem.
    pub fn capacidade(&self) -> usize { self.dados.len() }

    /// Quantas amostras esperam o consumidor.
    pub fn enfileiradas(&self) -> usize {
        let w = self.escrita.load(Ordering::Acquire);
        let r = self.leitura.load(Ordering::Acquire);
        (w - r) as usize
    }

    /// Quantas amostras ainda cabem.
    pub fn livres(&self) -> usize { self.capacidade() - self.enfileiradas() }

    /// Quantas amostras já foram escritas desde a criação.
    pub fn total_escrito(&self) -> u64 { self.escrita.load(Ordering::Acquire) }

    /// PRODUTOR: escreve o maior prefixo de QUADROS inteiros de `src` que cabe,
    /// e devolve quantas amostras entraram. Meio quadro no anel trocaria os
    /// canais de lugar para sempre.
    pub fn escrever_quadros(&self, src: &[f32], canais: usize) -> usize {
        if canais == 0 { return 0; }
        let w = self.escrita.load(Ordering::Relaxed);
        let r = self.leitura.load(Ordering::Acquire);
        let livres = self.capacidade() - (w - r) as usize;
        let n = src.len().min(livres) / canais * canais;
        for (k, v) in src[..n].iter().enumerate() {
            self.dados[((w + k as u64) & self.mascara) as usize].store(v.to_bits(), Ordering::Relaxed);
        }
        self.escrita.store(w + n as u64, Ordering::Release);
        n
    }

    /// CONSUMIDOR: lê até `dst.len()` amostras e devolve quantas leu. O resto de
    /// `dst` não é tocado.
    pub fn ler(&self, dst: &mut [f32]) -> usize {
        let r = self.leitura.load(Ordering::Relaxed);
        let w = self.escrita.load(Ordering::Acquire);
        let n = dst.len().min((w - r) as usize);
        for (k, slot) in dst[..n].iter_mut().enumerate() {
            *slot = f32::from_bits(self.dados[((r + k as u64) & self.mascara) as usize].load(Ordering::Relaxed));
        }
        self.leitura.store(r + n as u64, Ordering::Release);
        n
    }
}

/// O que o programa e a thread do dispositivo compartilham.
pub struct Compartilhado {
    /// As amostras a tocar.
    pub anel: Anel,
    /// Volume mestre, bits de um `f32`; aplicado na drenagem.
    pub volume: AtomicU32,
    /// Quadros entregues ao dispositivo desde a abertura (som ou silêncio).
    pub consumidos: AtomicU64,
    /// Drenagens que acharam o anel curto depois de ele já ter recebido som.
    pub faltas: AtomicU64,
}

impl Compartilhado {
    /// Estado novo com um anel de `amostras` e volume 1.
    pub fn new(amostras: usize) -> Compartilhado {
        Compartilhado {
            anel: Anel::new(amostras),
            volume: AtomicU32::new(1.0f32.to_bits()),
            consumidos: AtomicU64::new(0),
            faltas: AtomicU64::new(0),
        }
    }
}

/// CONSUMIDOR: enche `dst` (intercalado, `canais` por quadro) com o que houver
/// no anel × volume, e silêncio no resto. É a mesma função para o dispositivo
/// real e para o nulo, então o nulo mede o que o real faria.
pub fn drenar(c: &Compartilhado, dst: &mut [f32], canais: usize) {
    let n = c.anel.ler(dst);
    let volume = f32::from_bits(c.volume.load(Ordering::Relaxed));
    for v in &mut dst[..n] { *v *= volume; }
    if n < dst.len() {
        for v in &mut dst[n..] { *v = 0.0; }
        if c.anel.total_escrito() > 0 { c.faltas.fetch_add(1, Ordering::Relaxed); }
    }
    if canais > 0 { c.consumidos.fetch_add((dst.len() / canais) as u64, Ordering::Relaxed); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn anel_arredonda_para_potencia_de_dois() {
        assert_eq!(Anel::new(0).capacidade(), 2);
        assert_eq!(Anel::new(5).capacidade(), 8);
        assert_eq!(Anel::new(4096).capacidade(), 4096);
    }

    #[test]
    fn anel_escreve_e_le_na_ordem_e_da_a_volta() {
        let a = Anel::new(8);
        assert_eq!(a.escrever_quadros(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2), 6);
        let mut d = [0.0f32; 4];
        assert_eq!(a.ler(&mut d), 4);
        assert_eq!(d, [1.0, 2.0, 3.0, 4.0]);
        // 2 enfileiradas, 6 livres: a escrita passa pelo fim do vetor
        assert_eq!(a.escrever_quadros(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0], 2), 6);
        let mut e = [0.0f32; 8];
        assert_eq!(a.ler(&mut e), 8);
        assert_eq!(e, [5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
        assert_eq!(a.enfileiradas(), 0);
    }

    #[test]
    fn anel_cheio_aceita_so_quadros_inteiros() {
        let a = Anel::new(8);
        assert_eq!(a.escrever_quadros(&[0.5; 7], 2), 6, "7 amostras estéreo = 3 quadros");
        assert_eq!(a.livres(), 2);
        assert_eq!(a.escrever_quadros(&[0.5; 3], 2), 2, "só cabe 1 quadro");
        assert_eq!(a.escrever_quadros(&[0.5; 2], 2), 0, "cheio");
        assert_eq!(a.escrever_quadros(&[0.5; 4], 0), 0, "zero canais não escreve");
        assert_eq!(a.total_escrito(), 8);
    }

    #[test]
    fn drenar_aplica_volume_e_completa_com_silencio() {
        let c = Compartilhado::new(16);
        c.volume.store(0.5f32.to_bits(), Ordering::Relaxed);
        let mut d = [9.0f32; 4];
        drenar(&c, &mut d, 2);
        assert_eq!(d, [0.0; 4], "anel vazio: silêncio");
        assert_eq!(c.faltas.load(Ordering::Relaxed), 0, "falta só conta depois da primeira escrita");
        assert_eq!(c.consumidos.load(Ordering::Relaxed), 2, "2 quadros consumidos mesmo em silêncio");
        c.anel.escrever_quadros(&[1.0, -1.0], 2);
        let mut e = [9.0f32; 4];
        drenar(&c, &mut e, 2);
        assert_eq!(e, [0.5, -0.5, 0.0, 0.0]);
        assert_eq!(c.faltas.load(Ordering::Relaxed), 1, "faltou 1 quadro depois de já ter recebido som");
        assert_eq!(c.consumidos.load(Ordering::Relaxed), 4);
    }
}
