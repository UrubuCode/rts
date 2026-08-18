//! Hasher rápido para as chaves INTERNAS do motor — índices de nó e chaves de
//! cache de layout.
//!
//! O `SipHash` que a `std` usa por padrão é resistente a colisão adversária,
//! porque um `HashMap` da biblioteca padrão pode acabar recebendo chaves vindas
//! da rede. Nenhuma das chaves aqui vem de fora: são índices da arena e tuplas
//! de números que o próprio layout monta. Pagar SipHash por elas é pagar por uma
//! propriedade que não se usa — e paga-se muitas vezes: um layout de página
//! grande insere milhares de retângulos por nó e consulta os caches de medição
//! outras tantas.
//!
//! O algoritmo é o mesmo do `rustc` (FxHash): multiplicação por uma constante
//! ímpar e rotação, absorvendo 8 bytes por vez. Não é criptográfico e não deve
//! ser usado para nada que venha de fora do processo.

use std::hash::{BuildHasherDefault, Hasher};

/// Multiplicador ímpar de 64 bits (o mesmo do `rustc-hash`): espalha os bits
/// altos para os baixos, que é o que um `HashMap` lê primeiro.
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// Hasher de multiplicação-e-rotação. Ver a nota do módulo para quando NÃO usar.
#[derive(Default, Clone, Copy)]
pub struct FastHasher {
    hash: u64,
}

impl FastHasher {
    #[inline]
    fn absorb(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.absorb(u64::from_ne_bytes(c.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.absorb(u64::from_ne_bytes(buf));
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.absorb(n as u64);
    }
    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.absorb(n as u64);
    }
    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.absorb(n);
    }
    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.absorb(n as u64);
    }
}

/// `HashMap` com o hasher acima. Mesmo tipo e mesma API do da `std` — só o
/// hasher muda.
pub type FastMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FastHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = FastHasher::default();
        v.hash(&mut h);
        h.finish()
    }

    /// Determinístico e sensível ao valor — o mínimo que um hasher precisa
    /// entregar para um mapa responder certo.
    #[test]
    fn mesmo_valor_mesmo_hash_valores_diferentes_hashes_diferentes() {
        assert_eq!(hash_of(&42u64), hash_of(&42u64));
        assert_ne!(hash_of(&42u64), hash_of(&43u64));
        assert_ne!(hash_of(&(1u32, 2u32)), hash_of(&(2u32, 1u32)));
    }

    /// Índices vizinhos (o padrão de uma arena) não podem cair no mesmo balde —
    /// é exatamente o caso de uso, e um hasher que só somasse falharia aqui.
    #[test]
    fn indices_vizinhos_espalham() {
        let hashes: std::collections::HashSet<u64> =
            (0usize..1000).map(|i| hash_of(&i) >> 56).collect();
        assert!(hashes.len() > 100, "só {} baldes altos distintos", hashes.len());
    }

    #[test]
    fn funciona_como_mapa() {
        let mut m: FastMap<usize, &str> = FastMap::default();
        for i in 0..1000 {
            m.insert(i, "x");
        }
        assert_eq!(m.len(), 1000);
        assert_eq!(m.get(&999), Some(&"x"));
        assert_eq!(m.get(&1000), None);
    }
}
