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

/// Qual dos 4 `RenderPipeline` de partícula (`Scene3D::particle_pipeline_*`)
/// um lote usa — cruzamento de `aditivo` (o blend) com `tem_textura`
/// (`fs_particle`, disco procedural, vs. `fs_particle_tex`, que amostra
/// `albedo_tex`). Extraída como função PURA (sem `wgpu::Device`/`Scene3D`)
/// pra ser testável sem GPU: a decisão de qual pipeline não depende de nada
/// nativo, só de `aditivo`/`tem_textura` — só a ESCOLHA morre aqui, quem tem
/// os 4 `wgpu::RenderPipeline` de verdade é `render.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineParticula {
    Alfa,
    Aditivo,
    AlfaTex,
    AditivoTex,
}

/// `aditivo`: `modo == MODO_ADITIVO` (ver `scene_api::draw_particles*`).
/// `tem_textura`: `tex.is_some()` — só um lote de `drawParticlesTex` passa
/// `true` (`drawParticles` sempre enfileira sem textura, `tex: None`).
pub fn escolher_pipeline(aditivo: bool, tem_textura: bool) -> PipelineParticula {
    match (aditivo, tem_textura) {
        (false, false) => PipelineParticula::Alfa,
        (true, false) => PipelineParticula::Aditivo,
        (false, true) => PipelineParticula::AlfaTex,
        (true, true) => PipelineParticula::AditivoTex,
    }
}

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

    /// `drawParticlesTex` (tem textura) SEMPRE escolhe uma variante `*Tex` —
    /// é a garantia de que o caminho com textura de fato usa `fs_particle_tex`
    /// (amostra a textura) e não silenciosamente cai de volta no disco
    /// procedural de `drawParticles`.
    #[test]
    fn lote_com_textura_escolhe_variante_texturizada() {
        assert_eq!(escolher_pipeline(false, true), PipelineParticula::AlfaTex);
        assert_eq!(escolher_pipeline(true, true), PipelineParticula::AditivoTex);
    }

    /// `drawParticles` (sem textura) nunca escolhe uma variante `*Tex`.
    #[test]
    fn lote_sem_textura_nunca_escolhe_variante_texturizada() {
        assert_eq!(escolher_pipeline(false, false), PipelineParticula::Alfa);
        assert_eq!(escolher_pipeline(true, false), PipelineParticula::Aditivo);
    }

    /// As 4 combinações são distintas entre si — nenhuma colapsa noutra por
    /// engano (o `match` de `escolher_pipeline` é exaustivo por construção,
    /// mas isto fixa o contrato caso alguém troque por `if`s).
    #[test]
    fn as_quatro_combinacoes_sao_distintas() {
        let todas = [
            escolher_pipeline(false, false),
            escolher_pipeline(true, false),
            escolher_pipeline(false, true),
            escolher_pipeline(true, true),
        ];
        for i in 0..todas.len() {
            for j in (i + 1)..todas.len() {
                assert_ne!(todas[i], todas[j], "colisão entre índices {i} e {j}");
            }
        }
    }
}
