//! `gpu3d` — scene pass 3D real (malhas, câmera perspectiva, depth) renderizado
//! ANTES do pass do egui no mesmo encoder; o egui/DOM compõem por cima como
//! overlay. Primeira fatia da fase P7+ do design doc do egui.
//! Spec: `docs/specs/gpu3d-scene-pass.md`.
//!
//! Contrato imediato casando com o loop dirigido pelo TS: `mesh`/`camera`
//! persistem entre frames; `draw` enfileira instâncias do frame corrente e o
//! `endFrame` as consome. Vértices cruzam a ABI como handle de `buffer`
//! (f64 intercalado x,y,z,r,g,b — convertido a f32 no upload), nunca ponteiro.
//!
//! Só backend wgpu: no glow as chamadas são aceitas e ignoradas (mesma política
//! do `snapshot`).

mod math3d;
mod pipeline;

use std::collections::HashMap;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};
use AbiType::{F64, I64, U64};

use crate::ctx;
use crate::frame::Backend;

/// Uma malha residente na GPU (buffers de vértice e opcionalmente índice).
pub struct Mesh {
    pub vbuf: wgpu::Buffer,
    pub ibuf: Option<wgpu::Buffer>,
    pub vertex_count: u32,
    pub index_count: u32,
}

/// Uma instância enfileirada por `gpu3d.draw` neste frame (ângulos em rad).
pub struct DrawCmd {
    pub mesh_id: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
}

/// Estado 3D de UMA janela (vive em `RenderState.scene`, criado no 1º `mesh`).
pub struct SceneState {
    pub meshes: HashMap<i64, Mesh>,
    pub next_mesh_id: i64,
    pub draws: Vec<DrawCmd>,
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub fovy_rad: f32,
    pub near: f32,
    pub far: f32,
    pub clear_color: [f64; 3],
    /// Pipeline/uniforms/depth — criados junto com o estado (precisam do
    /// device + formato da surface, ambos disponíveis no 1º `mesh`).
    pub gpu: Option<pipeline::GpuRes>,
    /// Avisa UMA vez quando a fila estoura `MAX_DRAWS` (draws extras são
    /// descartados — nunca truncar silenciosamente, ver regra no-silent-caps).
    pub warned_overflow: bool,
}

impl SceneState {
    fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> SceneState {
        SceneState {
            meshes: HashMap::new(),
            next_mesh_id: 1,
            draws: Vec::new(),
            eye: [0.0, 0.0, 5.0],
            target: [0.0, 0.0, 0.0],
            fovy_rad: 60f32.to_radians(),
            near: 0.1,
            far: 1000.0,
            clear_color: [0.05, 0.06, 0.08],
            gpu: Some(pipeline::GpuRes::new(device, surface_format)),
            warned_overflow: false,
        }
    }
}

/// Grava o pass da cena (chamado por `present_wgpu` antes do pass do egui).
/// Retorna `true` se a cena foi desenhada — o pass do egui então usa
/// `LoadOp::Load` em vez de `Clear` para compor por cima.
pub(crate) fn record_if_active(
    scene: &mut Option<SceneState>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> bool {
    let Some(s) = scene.as_mut() else { return false };
    if s.draws.is_empty() {
        return false;
    }
    pipeline::record_scene_pass(s, device, queue, encoder, view, width, height);
    true
}

/// Roda `f` sobre o `SceneState` da janela, se existir (backend wgpu + cena já
/// criada pelo 1º `mesh`). No-op no glow ou sem cena — a política "aceita e
/// ignora" das chamadas gpu3d fora do wgpu.
fn with_scene(win: u64, f: impl FnOnce(&mut SceneState)) {
    ctx::with_ctx(win, |c| match &mut c.backend {
        Backend::Wgpu(r) => {
            if let Some(s) = r.scene.as_mut() {
                f(s);
            }
        }
        #[cfg(feature = "glow-backend")]
        Backend::Glow(_) => {}
    });
}

/// Lê `count` f64 little-endian de um handle de `buffer` (Entry::Buffer).
/// `None` se o handle não é Buffer ou é curto demais.
fn read_f64s(handle: u64, count: usize) -> Option<Vec<f64>> {
    rts_engine::heap::handles::with_entry(handle, |e| match e {
        Some(rts_engine::heap::handles::Entry::Buffer(b)) => {
            let need = count * 8;
            (b.len() >= need).then(|| {
                b[..need]
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                    .collect()
            })
        }
        _ => None,
    })
}

/// Lê `count` i32 little-endian de um handle de `buffer` (índices).
fn read_i32s(handle: u64, count: usize) -> Option<Vec<u32>> {
    rts_engine::heap::handles::with_entry(handle, |e| match e {
        Some(rts_engine::heap::handles::Entry::Buffer(b)) => {
            let need = count * 4;
            (b.len() >= need).then(|| {
                b[..need]
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()) as u32)
                    .collect()
            })
        }
        _ => None,
    })
}

/// Cria um `wgpu::Buffer` preenchido (mapped-at-creation; tamanho alinhado a 4).
fn gpu_buffer(device: &wgpu::Device, label: &str, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let size = (bytes.len() as u64 + 3) & !3;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: true,
    });
    let mut view = buf.get_mapped_range_mut(..);
    view.slice(..bytes.len()).copy_from_slice(bytes);
    drop(view);
    buf.unmap();
    buf
}

/// Upload comum de malha: vértices (e índices opcionais) já lidos dos buffers.
/// Retorna o meshId novo (>0).
fn upload_mesh(
    r: &mut crate::frame::RenderState,
    verts: Vec<f64>,
    vertex_count: u32,
    indices: Option<Vec<u32>>,
) -> i64 {
    // f64 intercalado (x,y,z,r,g,b) → f32 empacotado no layout do shader.
    let mut vbytes = Vec::with_capacity(verts.len() * 4);
    for v in &verts {
        vbytes.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    if r.scene.is_none() {
        r.scene = Some(SceneState::new(&r.device, r.config.format));
    }
    let scene = r.scene.as_mut().unwrap();
    let vbuf = gpu_buffer(&r.device, "rts-gpu3d vbuf", &vbytes, wgpu::BufferUsages::VERTEX);
    let (ibuf, index_count) = match indices {
        Some(idx) => {
            let mut ibytes = Vec::with_capacity(idx.len() * 4);
            for i in &idx {
                ibytes.extend_from_slice(&i.to_le_bytes());
            }
            let count = idx.len() as u32;
            (
                Some(gpu_buffer(&r.device, "rts-gpu3d ibuf", &ibytes, wgpu::BufferUsages::INDEX)),
                count,
            )
        }
        None => (None, 0),
    };
    let id = scene.next_mesh_id;
    scene.next_mesh_id += 1;
    scene.meshes.insert(id, Mesh { vbuf, ibuf, vertex_count, index_count });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_MESH(win: u64, buf: u64, vert_count: i64) -> i64 {
    if vert_count <= 0 {
        return 0;
    }
    let Some(verts) = read_f64s(buf, vert_count as usize * 6) else { return 0 };
    ctx::with_ctx(win, |c| match &mut c.backend {
        Backend::Wgpu(r) => upload_mesh(r, verts, vert_count as u32, None),
        #[cfg(feature = "glow-backend")]
        Backend::Glow(_) => 0,
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_MESH_INDEXED(
    win: u64,
    buf: u64,
    vert_count: i64,
    idx: u64,
    idx_count: i64,
) -> i64 {
    if vert_count <= 0 || idx_count <= 0 {
        return 0;
    }
    let Some(verts) = read_f64s(buf, vert_count as usize * 6) else { return 0 };
    let Some(indices) = read_i32s(idx, idx_count as usize) else { return 0 };
    if indices.iter().any(|i| *i >= vert_count as u32) {
        return 0; // índice fora do range de vértices = malha inválida
    }
    ctx::with_ctx(win, |c| match &mut c.backend {
        Backend::Wgpu(r) => upload_mesh(r, verts, vert_count as u32, Some(indices)),
        #[cfg(feature = "glow-backend")]
        Backend::Glow(_) => 0,
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_MESH_FREE(win: u64, mesh_id: i64) {
    with_scene(win, |s| {
        s.meshes.remove(&mesh_id);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_CAMERA(win: u64, ex: f64, ey: f64, ez: f64, tx: f64, ty: f64, tz: f64) {
    with_scene(win, |s| {
        s.eye = [ex as f32, ey as f32, ez as f32];
        s.target = [tx as f32, ty as f32, tz as f32];
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_PERSPECTIVE(win: u64, fov_y_deg: f64, near: f64, far: f64) {
    with_scene(win, |s| {
        if fov_y_deg > 0.0 && fov_y_deg < 180.0 {
            s.fovy_rad = (fov_y_deg as f32).to_radians();
        }
        if near > 0.0 && far > near {
            s.near = near as f32;
            s.far = far as f32;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_DRAW(
    win: u64,
    mesh_id: i64,
    x: f64,
    y: f64,
    z: f64,
    yaw_deg: f64,
    pitch_deg: f64,
    scale: f64,
) {
    with_scene(win, |s| {
        if s.draws.len() >= pipeline::MAX_DRAWS {
            if !s.warned_overflow {
                s.warned_overflow = true;
                eprintln!(
                    "rts-gpu3d: mais de {} draws num frame — extras descartados",
                    pipeline::MAX_DRAWS
                );
            }
            return;
        }
        s.draws.push(DrawCmd {
            mesh_id,
            x: x as f32,
            y: y as f32,
            z: z as f32,
            yaw: (yaw_deg as f32).to_radians(),
            pitch: (pitch_deg as f32).to_radians(),
            scale: scale as f32,
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU3D_CLEAR_COLOR(win: u64, red: f64, green: f64, blue: f64) {
    with_scene(win, |s| {
        s.clear_color = [red.clamp(0.0, 1.0), green.clamp(0.0, 1.0), blue.clamp(0.0, 1.0)];
    });
}

/// Helper de declaração de membro (mesmo shape do `func` de `lib.rs`).
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}

/// Registra o namespace `gpu3d` (chamado por `rts_egui::register`, doutrina
/// Registry — o engine nunca nomeia `gpu3d`).
pub fn register(e: &mut Engine) {
    e.ns("gpu3d")
        .doc("Real 3D scene pass under the egui overlay: meshes, perspective camera, depth. Spec: docs/specs/gpu3d-scene-pass.md")
        .member(func(
            "mesh",
            "__RTS_FN_NS_GPU3D_MESH",
            Sig::new(vec![U64, U64, I64], I64),
            "mesh(win: number, verts: number, vertCount: number): number",
            "Uploads a triangle-list mesh. `verts` = buffer handle with vertCount*6 f64 (x,y,z,r,g,b interleaved, colors 0..1). Returns meshId (>0) or 0 on error.",
            __RTS_FN_NS_GPU3D_MESH as *const u8,
        ))
        .member(func(
            "meshIndexed",
            "__RTS_FN_NS_GPU3D_MESH_INDEXED",
            Sig::new(vec![U64, U64, I64, U64, I64], I64),
            "meshIndexed(win: number, verts: number, vertCount: number, idx: number, idxCount: number): number",
            "Uploads an indexed mesh: `idx` = buffer handle with idxCount i32 indices (write_i32). Returns meshId (>0) or 0 on error.",
            __RTS_FN_NS_GPU3D_MESH_INDEXED as *const u8,
        ))
        .member(func(
            "meshFree",
            "__RTS_FN_NS_GPU3D_MESH_FREE",
            Sig::new(vec![U64, I64], AbiType::Void),
            "meshFree(win: number, meshId: number): void",
            "Frees the GPU buffers of a mesh.",
            __RTS_FN_NS_GPU3D_MESH_FREE as *const u8,
        ))
        .member(func(
            "camera",
            "__RTS_FN_NS_GPU3D_CAMERA",
            Sig::new(vec![U64, F64, F64, F64, F64, F64, F64], AbiType::Void),
            "camera(win: number, ex: number, ey: number, ez: number, tx: number, ty: number, tz: number): void",
            "Look-at camera: eye position + target point, up = +Y. Persists across frames.",
            __RTS_FN_NS_GPU3D_CAMERA as *const u8,
        ))
        .member(func(
            "perspective",
            "__RTS_FN_NS_GPU3D_PERSPECTIVE",
            Sig::new(vec![U64, F64, F64, F64], AbiType::Void),
            "perspective(win: number, fovYDeg: number, near: number, far: number): void",
            "Projection params (defaults 60deg, 0.1, 1000). Aspect ratio tracks the window size automatically.",
            __RTS_FN_NS_GPU3D_PERSPECTIVE as *const u8,
        ))
        .member(func(
            "draw",
            "__RTS_FN_NS_GPU3D_DRAW",
            Sig::new(vec![U64, I64, F64, F64, F64, F64, F64, F64], AbiType::Void),
            "draw(win: number, meshId: number, x: number, y: number, z: number, yawDeg: number, pitchDeg: number, scale: number): void",
            "Queues one instance of the mesh this frame (model = T*Ry(yaw)*Rx(pitch)*S). endFrame renders the scene BEFORE the egui pass and clears the queue.",
            __RTS_FN_NS_GPU3D_DRAW as *const u8,
        ))
        .member(func(
            "clearColor",
            "__RTS_FN_NS_GPU3D_CLEAR_COLOR",
            Sig::new(vec![U64, F64, F64, F64], AbiType::Void),
            "clearColor(win: number, r: number, g: number, b: number): void",
            "Scene background color (components 0..1). Only used on frames where draw() was called.",
            __RTS_FN_NS_GPU3D_CLEAR_COLOR as *const u8,
        ))
        .done();
}
