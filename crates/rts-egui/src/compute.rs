//! `rts:gpu` — compute shaders WGSL chamáveis dos scripts TS.
//!
//! A GPU do runtime só era usada para render (egui/scene3d). Este namespace
//! expõe o MESMO `wgpu::Device` compartilhado (`frame::gpu::shared_gpu`) para
//! trabalho de dados: o script compila um kernel WGSL, sobe bytes de um
//! `rts:buffer` para um storage buffer, despacha e lê o resultado de volta.
//!
//! Fluxo típico:
//! ```ts
//! import gpu from "rts:gpu";
//! const pipe = gpu.shader(`@group(0) @binding(0) var<storage, read_write> d: array<f32>;
//!   @compute @workgroup_size(64)
//!   fn main(@builtin(global_invocation_id) id: vec3<u32>) { d[id.x] = d[id.x] * 2.0; }`);
//! const buf = gpu.buffer(n * 4);
//! gpu.write(buf, cpuBuf, n * 4);      // rts:buffer -> GPU
//! gpu.bind_buffer(pipe, 0, buf);
//! gpu.dispatch(pipe, n / 64, 1, 1);   // submete (assíncrono)
//! gpu.read(buf, cpuBuf, n * 4);       // sincroniza e traz de volta
//! ```
//!
//! Decisões:
//! - `dispatch` só SUBMETE (não espera). Quem sincroniza é `read` — o padrão
//!   "dados moram na GPU, lê-se só o necessário" é o único em que compute
//!   compensa; um write→dispatch→read completo por frame paga ~ms de round-trip
//!   e perde da CPU em cargas pequenas.
//! - Device criado sob demanda e SEM janela (headless): `shared_gpu` já pede o
//!   adapter com `compatible_surface: None`. Se uma janela existir, reusa o
//!   device dela (e vice-versa). Quando o compute cria primeiro, pede
//!   high_perf + high_limits — limites downlevel valem para UI 2D, não para
//!   storage buffers de física.
//! - Estado em `thread_local!` como todo o rts-egui (wgpu aqui é !Send por
//!   contrato do runtime: uma thread de TS).
//! - Erro de WGSL não derruba o runtime: `shader` devolve 0 e o texto do
//!   validador sai no stderr (capturado por error scope).

use std::cell::RefCell;
use std::collections::HashMap;

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::heap::handles::{Entry, with_entry, with_entry_mut};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::frame::gpu::{GpuConfig, SharedGpu, shared_gpu};

/// Um pipeline compilado + os buffers ligados aos `@binding(slot)` do group 0.
struct Pipe {
    pipeline: wgpu::ComputePipeline,
    /// (slot, id de buffer em `Ctx::buffers`) — validado no `dispatch`.
    binds: Vec<(u32, u64)>,
}

/// Leitura em voo (read_begin/read_poll): staging mapeando em background.
struct Pending {
    staging: wgpu::Buffer,
    bytes: u64,
    rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

struct Ctx {
    gpu: SharedGpu,
    pipes: HashMap<u64, Pipe>,
    buffers: HashMap<u64, wgpu::Buffer>,
    pending: HashMap<u64, Pending>,
    next: u64,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Roda `f` com o contexto de compute, criando device (headless) na 1ª chamada.
/// `default` quando não há GPU — o script vê 0/-1, nunca um crash.
fn with_gpu<R>(default: R, f: impl FnOnce(&mut Ctx) -> R) -> R {
    CTX.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.is_none() {
            // bit0 high_perf + bit2 high_limits: compute quer o GPU discreto e
            // limites reais. Sem efeito se o device já nasceu por uma janela.
            match shared_gpu(GpuConfig::from_bits(0b0101)) {
                Ok(gpu) => {
                    *b = Some(Ctx {
                        gpu,
                        pipes: HashMap::new(),
                        buffers: HashMap::new(),
                        pending: HashMap::new(),
                        next: 1,
                    });
                }
                Err(e) => {
                    eprintln!("[rts:gpu] sem device: {e}");
                    return default;
                }
            }
        }
        f(b.as_mut().expect("ctx recém-criado"))
    })
}

/// 1 se há GPU utilizável (cria o device na primeira consulta).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_AVAILABLE() -> I64 {
    with_gpu(0, |_| 1)
}

/// Compila um kernel WGSL (entry point `main`). Handle do pipeline, 0 em erro
/// de validação (mensagem no stderr).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_SHADER(src_ptr: *const u8, src_len: i64) -> Handle {
    let Some(src) = (unsafe { rts_engine::abi::str_abi::from_abi(src_ptr, src_len) }) else {
        return 0;
    };
    with_gpu(0, |c| {
        let dev = &c.gpu.device;
        // Error scope: WGSL inválido vira Err aqui em vez de panic global.
        let scope = dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rts:gpu shader"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rts:gpu pipeline"),
            layout: None,
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            eprintln!("[rts:gpu] shader inválido: {e}");
            return 0;
        }
        let id = c.next;
        c.next += 1;
        c.pipes.insert(
            id,
            Pipe {
                pipeline,
                binds: Vec::new(),
            },
        );
        id
    })
}

/// Storage buffer de `bytes` na GPU. Handle, 0 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BUFFER(bytes: I64) -> Handle {
    if bytes <= 0 {
        return 0;
    }
    with_gpu(0, |c| {
        let buf = c.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts:gpu storage"),
            size: bytes as u64,
            // VERTEX: o scene3d pode bindar este buffer como vertex buffer de
            // instância (render instanciado da água — zero readback). Caminho de
            // vertex buffer e não storage-no-vertex-stage: downlevel permite 0
            // storage buffers no estágio vertex, vertex buffer funciona sempre.
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        let id = c.next;
        c.next += 1;
        c.buffers.insert(id, buf);
        id
    })
}

/// Sobe `bytes` do `rts:buffer` `src` para o buffer de GPU. 1 ok, 0 erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_WRITE(gbuf: U64, src: U64, bytes: I64) -> I64 {
    let Some(data) = with_entry(src, |e| match e {
        Some(Entry::Buffer(b)) => Some(b[..(bytes as usize).min(b.len())].to_vec()),
        _ => None,
    }) else {
        return 0;
    };
    with_gpu(0, |c| {
        let Some(buf) = c.buffers.get(&gbuf) else {
            return 0;
        };
        c.gpu.queue.write_buffer(buf, 0, &data);
        1
    })
}

/// Liga o buffer `gbuf` ao `@binding(slot)` do `@group(0)` do pipeline. 1 ok.
///
/// Chama-se `bind_buffer` (não `bind`): o lowering intercepta `.bind(` como
/// `Function.prototype.bind` ANTES de resolver membro de namespace, e o script
/// morre com "unbound identifier `gpu`". Membro de namespace não pode se chamar
/// `bind`/`call`/`apply` enquanto isso for assim.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BIND(pipe: U64, slot: I64, gbuf: U64) -> I64 {
    with_gpu(0, |c| {
        if !c.buffers.contains_key(&gbuf) {
            return 0;
        }
        let Some(p) = c.pipes.get_mut(&pipe) else {
            return 0;
        };
        p.binds.retain(|(s, _)| *s != slot as u32);
        p.binds.push((slot as u32, gbuf));
        1
    })
}

/// Submete `gx × gy × gz` workgroups do pipeline. NÃO espera a GPU terminar —
/// `read` é quem sincroniza. 1 ok, 0 erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_DISPATCH(pipe: U64, gx: I64, gy: I64, gz: I64) -> I64 {
    if gx <= 0 || gy <= 0 || gz <= 0 {
        return 0;
    }
    with_gpu(0, |c| {
        let Some(p) = c.pipes.get(&pipe) else {
            return 0;
        };
        let entries: Vec<wgpu::BindGroupEntry> = p
            .binds
            .iter()
            .filter_map(|(slot, id)| {
                c.buffers.get(id).map(|b| wgpu::BindGroupEntry {
                    binding: *slot,
                    resource: b.as_entire_binding(),
                })
            })
            .collect();
        let dev = &c.gpu.device;
        // Bind group inconsistente com o layout do shader (slot faltando, tipo
        // errado) é erro de VALIDAÇÃO — capturado aqui, 0 pro script.
        let scope = dev.push_error_scope(wgpu::ErrorFilter::Validation);
        let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rts:gpu bind group"),
            layout: &p.pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rts:gpu dispatch"),
        });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rts:gpu pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&p.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(gx as u32, gy as u32, gz as u32);
        }
        c.gpu.queue.submit([enc.finish()]);
        if let Some(e) = pollster::block_on(scope.pop()) {
            eprintln!("[rts:gpu] dispatch inválido: {e}");
            return 0;
        }
        1
    })
}

/// Lê `bytes` do buffer de GPU para o `rts:buffer` `dst`. SINCRONIZA (espera
/// todo trabalho submetido terminar). Bytes lidos, -1 erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ(gbuf: U64, dst: U64, bytes: I64) -> I64 {
    if bytes <= 0 {
        return -1;
    }
    let data: Option<Vec<u8>> = with_gpu(None, |c| {
        let Some(buf) = c.buffers.get(&gbuf) else {
            return None;
        };
        let n = (bytes as u64).min(buf.size());
        let dev = &c.gpu.device;
        // Staging MAP_READ: storage buffer não é mapeável direto.
        let staging = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts:gpu readback"),
            size: n,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rts:gpu read"),
        });
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, n);
        c.gpu.queue.submit([enc.finish()]);
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = dev.poll(wgpu::PollType::wait_indefinitely());
        match rx.recv() {
            Ok(Ok(())) => {}
            _ => return None,
        }
        let out = slice.get_mapped_range().to_vec();
        staging.unmap();
        Some(out)
    });
    let Some(data) = data else {
        return -1;
    };
    with_entry_mut(dst, |e| match e {
        Some(Entry::Buffer(b)) => {
            let n = data.len().min(b.len());
            b[..n].copy_from_slice(&data[..n]);
            n as i64
        }
        _ => -1,
    })
}

/// LEITURA ASSÍNCRONA, passo 1: agenda a cópia GPU→staging e o map, SEM
/// esperar. Devolve um ticket (0 = erro). A física-como-serviço nasce aqui:
/// o jogo agenda no fim do frame e pergunta nos seguintes com `read_poll`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ_BEGIN(gbuf: U64, bytes: I64) -> Handle {
    if bytes <= 0 {
        return 0;
    }
    with_gpu(0, |c| {
        let Some(buf) = c.buffers.get(&gbuf) else {
            return 0;
        };
        let n = (bytes as u64).min(buf.size());
        let dev = &c.gpu.device;
        let staging = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts:gpu readback async"),
            size: n,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rts:gpu read async"),
        });
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, n);
        c.gpu.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let id = c.next;
        c.next += 1;
        c.pending.insert(
            id,
            Pending {
                staging,
                bytes: n,
                rx,
            },
        );
        id
    })
}

/// LEITURA ASSÍNCRONA, passo 2: pergunta SEM bloquear. Pronto → copia para o
/// rts:buffer `dst`, consome o ticket e devolve os bytes. Ainda em voo → 0.
/// Erro/ticket inválido → -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ_POLL(ticket: U64, dst: U64) -> I64 {
    let data: i64 = with_gpu(-1, |c| {
        if !c.pending.contains_key(&ticket) {
            return -1;
        }
        // Poll NÃO-bloqueante: avança os callbacks sem esperar a GPU.
        let _ = c.gpu.device.poll(wgpu::PollType::Poll);
        let done = match c.pending.get(&ticket) {
            Some(p) => match p.rx.try_recv() {
                Ok(Ok(())) => 1,
                Ok(Err(_)) => 2,
                // Empty = ainda em voo; DISCONNECTED = o callback morreu sem
                // responder — tratar como em-voo travava o ticket PARA SEMPRE
                // (simulacao congelada com o jogo rodando; visto ao vivo).
                Err(std::sync::mpsc::TryRecvError::Empty) => 0,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => 2,
            },
            None => 2,
        };
        if done == 0 {
            return 0;
        }
        let p = c.pending.remove(&ticket).expect("checado acima");
        if done == 2 {
            return -1;
        }
        let out = p.staging.slice(..).get_mapped_range().to_vec();
        p.staging.unmap();
        let n = with_entry_mut(dst, |e| match e {
            Some(Entry::Buffer(b)) => {
                let n = out.len().min(b.len());
                b[..n].copy_from_slice(&out[..n]);
                n as i64
            }
            _ => -1,
        });
        n
    });
    data
}

/// Libera um buffer de GPU. 1 se existia.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BUFFER_FREE(gbuf: U64) -> I64 {
    with_gpu(0, |c| {
        // Também some dos binds de todo pipeline — um dispatch posterior com o
        // slot vazio falha a validação em vez de usar buffer morto.
        for p in c.pipes.values_mut() {
            p.binds.retain(|(_, id)| *id != gbuf);
        }
        i64::from(c.buffers.remove(&gbuf).is_some())
    })
}

/// Handle CLONADO (Arc) do buffer `id`, para o scene3d bindar direto como
/// vertex buffer de instância — mesmo device (`shared_gpu`), zero readback.
pub(crate) fn buffer_handle(id: u64) -> Option<wgpu::Buffer> {
    with_gpu(None, |c| c.buffers.get(&id).cloned())
}

/// Nome do adapter (debug/telemetria). Handle de string GC, 0 sem GPU.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_ADAPTER_NAME() -> Handle {
    unsafe extern "C" {
        fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    }
    with_gpu(0, |c| {
        let info = c.gpu.adapter.get_info();
        let name = info.name;
        unsafe { __RTS_FN_NS_GC_STRING_NEW(name.as_ptr(), name.len() as i64) }
    })
}

// ---------------------------------------------------------------------------
// Registro (builder hand-written, padrão rts:audio)
// ---------------------------------------------------------------------------

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

/// Registra a namespace `gpu` no motor.
pub fn register(e: &mut Engine) {
    e.ns("gpu")
        .doc("Compute WGSL na GPU compartilhada do runtime (headless ou junto do render).")
        .member(func(
            "available",
            "__RTS_FN_NS_GPU_AVAILABLE",
            Sig::new(Vec::new(), AbiType::I64),
            "available(): number",
            "1 se há GPU utilizável (cria o device headless na primeira consulta).",
            __RTS_FN_NS_GPU_AVAILABLE as *const u8,
        ))
        .member(func(
            "shader",
            "__RTS_FN_NS_GPU_SHADER",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "shader(wgsl: string): number",
            "Compila um kernel WGSL (entry point `main`). Handle do pipeline; 0 = erro de validação (stderr).",
            __RTS_FN_NS_GPU_SHADER as *const u8,
        ))
        .member(func(
            "buffer",
            "__RTS_FN_NS_GPU_BUFFER",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "buffer(bytes: number): number",
            "Storage buffer na GPU. Handle; 0 = erro.",
            __RTS_FN_NS_GPU_BUFFER as *const u8,
        ))
        .member(func(
            "write",
            "__RTS_FN_NS_GPU_WRITE",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "write(gbuf: number, src: number, bytes: number): number",
            "Sobe bytes de um rts:buffer para o buffer de GPU. 1 ok, 0 erro.",
            __RTS_FN_NS_GPU_WRITE as *const u8,
        ))
        .member(func(
            "bind_buffer",
            "__RTS_FN_NS_GPU_BIND",
            Sig::new(vec![AbiType::U64, AbiType::I64, AbiType::U64], AbiType::I64),
            "bind_buffer(pipe: number, slot: number, gbuf: number): number",
            "Liga o buffer ao @binding(slot) do @group(0) do pipeline. 1 ok.",
            __RTS_FN_NS_GPU_BIND as *const u8,
        ))
        .member(func(
            "dispatch",
            "__RTS_FN_NS_GPU_DISPATCH",
            Sig::new(
                vec![AbiType::U64, AbiType::I64, AbiType::I64, AbiType::I64],
                AbiType::I64,
            ),
            "dispatch(pipe: number, gx: number, gy: number, gz: number): number",
            "Submete gx*gy*gz workgroups (assíncrono — `read` sincroniza). 1 ok.",
            __RTS_FN_NS_GPU_DISPATCH as *const u8,
        ))
        .member(func(
            "read",
            "__RTS_FN_NS_GPU_READ",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "read(gbuf: number, dst: number, bytes: number): number",
            "Espera a GPU e copia bytes do buffer de GPU para um rts:buffer. Bytes lidos, -1 erro.",
            __RTS_FN_NS_GPU_READ as *const u8,
        ))
        .member(func(
            "read_begin",
            "__RTS_FN_NS_GPU_READ_BEGIN",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Handle),
            "read_begin(gbuf: number, bytes: number): number",
            "Async read, step 1: schedules the GPU->staging copy WITHOUT waiting; returns a ticket (0 = error). Poll with read_poll.",
            __RTS_FN_NS_GPU_READ_BEGIN as *const u8,
        ))
        .member(func(
            "read_poll",
            "__RTS_FN_NS_GPU_READ_POLL",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::I64),
            "read_poll(ticket: number, dst: number): number",
            "Async read, step 2: non-blocking check. Ready -> copies into the rts:buffer dst, consumes the ticket, returns bytes. In flight -> 0. Error -> -1.",
            __RTS_FN_NS_GPU_READ_POLL as *const u8,
        ))
        .member(func(
            "buffer_free",
            "__RTS_FN_NS_GPU_BUFFER_FREE",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "buffer_free(gbuf: number): number",
            "Libera um buffer de GPU (e o desliga de todos os pipelines). 1 se existia.",
            __RTS_FN_NS_GPU_BUFFER_FREE as *const u8,
        ))
        .member(func(
            "adapter_name",
            "__RTS_FN_NS_GPU_ADAPTER_NAME",
            Sig::new(Vec::new(), AbiType::Handle),
            "adapter_name(): string",
            "Nome do adapter de GPU em uso (debug).",
            __RTS_FN_NS_GPU_ADAPTER_NAME as *const u8,
        ))
        .done();
}
