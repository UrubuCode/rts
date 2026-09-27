//! Layout do buffer de instância de `drawParticles`/`drawParticlesTex`: 9 f32
//! por partícula. O TS escreve; este arquivo só nomeia os offsets e lê.
//!
//! POLÍTICA (pool, emissão, forma, curvas) fica no rts-game em TypeScript —
//! aqui só o suficiente para expandir cada linha num quad billboard na GPU.

pub const PART_X: usize = 0;
// Y, Z, TAM e R, G, B só documentam a posição dentro dos vec4 de instância
// (ver `pipeline::particle_ibl`/`vs_particle`); nenhum código Rust os indexa
// individualmente — os floats atravessam intactos até o shader.
#[allow(dead_code)]
pub const PART_Y: usize = 1;
#[allow(dead_code)]
pub const PART_Z: usize = 2;
#[allow(dead_code)]
pub const PART_TAM: usize = 3;
pub const PART_ROT: usize = 4;
#[allow(dead_code)]
pub const PART_R: usize = 5;
#[allow(dead_code)]
pub const PART_G: usize = 6;
#[allow(dead_code)]
pub const PART_B: usize = 7;
pub const PART_A: usize = 8;
pub const PART_FLOATS: usize = 9;

/// Modos de `drawParticles` (o 3º/4º parâmetro `modo`); qualquer outro valor
/// cai em ALFA — nunca um pipeline inválido por um `modo` errado do chamador.
pub const MODO_ALFA: i64 = 0;
pub const MODO_ADITIVO: i64 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_tem_9_floats_e_offsets_sem_sobreposicao() {
        let offsets = [PART_X, PART_Y, PART_Z, PART_TAM, PART_ROT, PART_R, PART_G, PART_B, PART_A];
        let mut vistos = offsets.to_vec();
        vistos.sort();
        assert_eq!(vistos, (0..PART_FLOATS).collect::<Vec<_>>());
    }

    #[test]
    fn modo_desconhecido_nao_e_aditivo() {
        // Qualquer chamador com modo=2, -1 etc. deve cair no caminho alfa: o
        // teste de superfície (Task 1, ui_surface.rs) confere que scene_api
        // trata assim; aqui só documentamos os dois valores válidos.
        assert_ne!(MODO_ALFA, MODO_ADITIVO);
    }
}
