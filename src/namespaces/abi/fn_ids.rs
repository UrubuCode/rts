//! Tabela de `fn_id` usada pelo `__rts_dispatch`.
//!
//! Cada constante mapeia uma operacao de runtime para o valor que o codegen
//! emite como primeiro argumento do dispatch. Novos ids devem:
//!   1. receber um valor sequencial
//!   2. bumpar `FN_ID_COUNT`
//!   3. ganhar uma linha em `fn_id_label` (senao aparecem como "unknown"
//!      em `--dump-statistics`).

// --- fn_id constants para __rts_dispatch ---
// Slot layout: __rts_dispatch(fn_id, a0, a1, a2, a3, a4, a5) -> i64
pub(crate) const FN_RESET_THREAD_STATE: i64 = 0;
pub(crate) const FN_BIND_IDENTIFIER: i64 = 1; // (ptr, len, handle, mutable)
pub(crate) const FN_BOX_STRING: i64 = 2; // (ptr, len)
pub(crate) const FN_BOX_BOOL: i64 = 3; // (flag)
pub(crate) const FN_EVAL_EXPR: i64 = 4; // (ptr, len)
pub(crate) const FN_EVAL_STMT: i64 = 5; // (ptr, len)
pub(crate) const FN_READ_IDENTIFIER: i64 = 6; // (ptr, len)
pub(crate) const FN_BINOP: i64 = 7; // (op, lhs, rhs)
pub(crate) const FN_IS_TRUTHY: i64 = 8; // (handle)
pub(crate) const FN_UNBOX_NUMBER: i64 = 9; // (handle)
pub(crate) const FN_BOX_NUMBER: i64 = 10; // (bits as i64)
pub(crate) const FN_IO_PRINT: i64 = 11;
pub(crate) const FN_IO_STDOUT_WRITE: i64 = 12;
pub(crate) const FN_IO_STDERR_WRITE: i64 = 13;
pub(crate) const FN_IO_PANIC: i64 = 14;
pub(crate) const FN_CRYPTO_SHA256: i64 = 15;
pub(crate) const FN_PROCESS_EXIT: i64 = 16;
pub(crate) const FN_GLOBAL_SET: i64 = 17; // (key, value)
pub(crate) const FN_GLOBAL_GET: i64 = 18;
pub(crate) const FN_GLOBAL_HAS: i64 = 19;
pub(crate) const FN_GLOBAL_DELETE: i64 = 20;
pub(crate) const FN_BOX_NATIVE_FN: i64 = 21; // (ptr, len) -> handle to NativeFunction
pub(crate) const FN_CALL_BY_HANDLE: i64 = 22; // (fn_handle, argc, a0..a5) -> i64
pub(crate) const FN_NEW_INSTANCE: i64 = 23; // (class_ptr, class_len) -> object_handle (fields vazios)
pub(crate) const FN_LOAD_FIELD: i64 = 24; // (obj_handle, field_ptr, field_len) -> value_handle
pub(crate) const FN_STORE_FIELD: i64 = 25; // (obj_handle, field_ptr, field_len, value_handle) -> 1/0
pub(crate) const FN_PIN_HANDLE: i64 = 26; // (handle) -> handle
pub(crate) const FN_UNPIN_HANDLE: i64 = 27; // (handle) -> handle
pub(crate) const FN_COMPACT_EXCLUDING: i64 = 28; // (handle) -> freed count

/// Numero total de FN_* distintos. Usado como tamanho dos arrays de metricas
/// por-fn_id em `RuntimeMetrics`.
pub(crate) const FN_ID_COUNT: usize = 29;

/// Mapeia `fn_id` para nome legivel. Usado pela renderizacao de
/// `--dump-statistics` para mostrar tempo gasto em cada ponto de dispatch
/// separadamente. Indices fora do range retornam `"unknown"`.
pub fn fn_id_label(fn_id: i64) -> &'static str {
    match fn_id {
        0 => "reset_thread_state",
        1 => "bind_identifier",
        2 => "box_string",
        3 => "box_bool",
        4 => "eval_expr",
        5 => "eval_stmt",
        6 => "read_identifier",
        7 => "binop",
        8 => "is_truthy",
        9 => "unbox_number",
        10 => "box_number",
        11 => "io.print",
        12 => "io.stdout_write",
        13 => "io.stderr_write",
        14 => "io.panic",
        15 => "crypto.sha256",
        16 => "process.exit",
        17 => "globals.set",
        18 => "globals.get",
        19 => "globals.has",
        20 => "globals.remove",
        21 => "box_native_fn",
        22 => "call_by_handle",
        23 => "new_instance",
        24 => "load_field",
        25 => "store_field",
        26 => "pin_handle",
        27 => "unpin_handle",
        28 => "compact_excluding",
        _ => "unknown",
    }
}
