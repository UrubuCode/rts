//! A casca do `rts:gpu` para o motor ANTIGO.
//!
//! Mesma divisão que `super`: a lógica está em `crate::compute`, em Rust comum,
//! e aqui ficam os símbolos de linker e a tabela do namespace. A tradução que
//! esta casca faz é de ONDE VÊM E PARA ONDE VÃO OS BYTES — `crate::compute` só
//! sobe e baixa, e quem sabe o que um handle de `rts:buffer` significa é este
//! motor.

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::heap::handles::{Entry, with_entry, with_entry_mut};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::compute::{self, Poll};

/// Os bytes de um `rts:buffer`, do offset dado.
fn source(src: u64, offset: i64, bytes: i64) -> Option<Vec<u8>> {
    if bytes <= 0 || offset < 0 {
        return None;
    }
    with_entry(src, |entry| match entry {
        Some(Entry::Buffer(held)) => {
            let start = offset as usize;
            let end = start.saturating_add(bytes as usize).min(held.len());
            (start < end).then(|| held[start..end].to_vec())
        }
        _ => None,
    })
}

/// Escreve num `rts:buffer` e responde quantos bytes couberam.
fn sink(dst: u64, data: &[u8]) -> i64 {
    with_entry_mut(dst, |entry| match entry {
        Some(Entry::Buffer(held)) => {
            let n = data.len().min(held.len());
            held[..n].copy_from_slice(&data[..n]);
            n as i64
        }
        _ => -1,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_AVAILABLE() -> I64 {
    i64::from(compute::available())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_SHADER(src_ptr: *const u8, src_len: i64) -> Handle {
    match unsafe { rts_engine::abi::str_abi::from_abi(src_ptr, src_len) } {
        Some(source) => compute::shader(source),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BUFFER(bytes: I64) -> Handle {
    compute::buffer(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_WRITE(gbuf: U64, src: U64, bytes: I64) -> I64 {
    match source(src, 0, bytes) {
        Some(data) => i64::from(compute::write(gbuf, &data)),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_WRITE_AT(
    gbuf: U64,
    src: U64,
    src_off: I64,
    dst_off: I64,
    bytes: I64,
) -> I64 {
    match source(src, src_off, bytes) {
        // Curto de propósito: a superfície antiga recusava uma origem que não
        // cobrisse `bytes` inteiros, e `source` já responde `None` para isso.
        Some(data) if data.len() == bytes.max(0) as usize => {
            i64::from(compute::write_at(gbuf, dst_off, &data))
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BIND(pipe: U64, slot: I64, gbuf: U64) -> I64 {
    i64::from(compute::bind_buffer(pipe, slot, gbuf))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_DISPATCH(pipe: U64, gx: I64, gy: I64, gz: I64) -> I64 {
    i64::from(compute::dispatch(pipe, gx, gy, gz))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ(gbuf: U64, dst: U64, bytes: I64) -> I64 {
    match compute::read(gbuf, bytes) {
        Some(data) => {
            let escritos = sink(dst, &data);
            if escritos < 0 {
                // A leitura da GPU funcionou e o DESTINO não é um buffer. Dito
                // aqui porque o chamador recebe o mesmo `-1` nos dois casos e
                // não tem como saber qual foi — e um deles significa que um
                // handle de buffer deixou de resolver no meio da execução.
                eprintln!(
                    "[rts:gpu] leitura ok ({} bytes) mas o destino {dst} nao e um buffer",
                    data.len()
                );
            }
            escritos
        }
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ_BEGIN(gbuf: U64, bytes: I64) -> Handle {
    compute::read_begin(gbuf, bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_READ_POLL(ticket: U64, dst: U64) -> I64 {
    match compute::read_poll(ticket) {
        Poll::Pending => 0,
        Poll::Failed => -1,
        Poll::Done(data) => sink(dst, &data),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_BUFFER_FREE(gbuf: U64) -> I64 {
    i64::from(compute::buffer_free(gbuf))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GPU_ADAPTER_NAME() -> Handle {
    use rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW;
    match compute::adapter_name() {
        Some(name) => unsafe { __RTS_FN_NS_GC_STRING_NEW(name.as_ptr(), name.len() as i64) },
        None => 0,
    }
}

/// Solta tudo que o `rts:gpu` segura, na ordem, enquanto o processo está
/// inteiro — reexportado aqui porque o binário do motor antigo chama
/// `ns::gpu::shutdown()` na saída, e este módulo É o `ns::gpu` dele.
///
/// Ver `crate::compute::shutdown` para por que não pode ficar para o destrutor
/// de thread-local.
pub fn shutdown() {
    compute::shutdown();
}

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
        ret_class: None,
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
            "write_at",
            "__RTS_FN_NS_GPU_WRITE_AT",
            Sig::new(
                vec![AbiType::U64, AbiType::U64, AbiType::I64, AbiType::I64, AbiType::I64],
                AbiType::I64,
            ),
            "write_at(gbuf: number, src: number, srcOff: number, dstOff: number, bytes: number): number",
            "Partial write with offsets: copies bytes from the rts:buffer into the GPU buffer at dstOff. Enables poking ONE body without re-uploading full state. 1 ok, 0 error.",
            __RTS_FN_NS_GPU_WRITE_AT as *const u8,
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
