use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use cranelift_codegen::ir::{
    AbiParam, InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};

/// Tracks whether a VReg holds a native value or an opaque handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VRegKind {
    Handle,    // i64 handle to ValueStore
    NativeF64, // raw f64 bits stored as i64
    NativeI32, // raw i32 value stored as i64
}

#[derive(Debug, Clone, Copy)]
struct BindingState {
    slot: StackSlot,
    mutable: bool,
    /// Kind do valor guardado no slot. Para NativeF64/NativeI32, o slot
    /// armazena os bits crus; LoadBinding re-emite com o mesmo VRegKind
    /// para manter o caminho nativo em BinOps subsequentes.
    kind: VRegKind,
}

use crate::mir::{MirBinOp, MirInstruction, MirUnaryOp, TypedMirFunction, VReg};

const ABI_ARG_SLOTS: usize = 6;
const ABI_PARAM_COUNT: usize = ABI_ARG_SLOTS + 1;
const ABI_UNDEFINED_HANDLE: i64 = 0;

use crate::namespaces::abi::{
    FN_BIND_IDENTIFIER, FN_BINOP, FN_BOX_BOOL, FN_BOX_NATIVE_FN, FN_BOX_NUMBER, FN_BOX_STRING,
    FN_CALL_BY_HANDLE, FN_CRYPTO_SHA256, FN_EVAL_STMT, FN_GLOBAL_DELETE, FN_GLOBAL_GET,
    FN_GLOBAL_HAS, FN_GLOBAL_SET, FN_IO_PANIC, FN_IO_PRINT, FN_IO_STDERR_WRITE,
    FN_COMPACT_EXCLUDING, FN_IO_STDOUT_WRITE, FN_IS_TRUTHY, FN_LOAD_FIELD, FN_NEW_INSTANCE,
    FN_PIN_HANDLE, FN_PROCESS_EXIT, FN_READ_IDENTIFIER, FN_STORE_FIELD, FN_UNBOX_NUMBER,
    FN_UNPIN_HANDLE,
};

const RTS_DISPATCH: &str = "__rts_dispatch";

/// Mapeamento de callee conhecido para fn_id do __rts_dispatch.
/// O codegen emite __rts_dispatch(fn_id, args...) diretamente, sem lookup por string.
const CALLEE_FN_IDS: &[(&str, i64)] = &[
    ("io.print", FN_IO_PRINT),
    ("io.stdout_write", FN_IO_STDOUT_WRITE),
    ("io.stderr_write", FN_IO_STDERR_WRITE),
    ("io.panic", FN_IO_PANIC),
    ("crypto.sha256", FN_CRYPTO_SHA256),
    ("process.exit", FN_PROCESS_EXIT),
    ("global.set", FN_GLOBAL_SET),
    ("global.get", FN_GLOBAL_GET),
    ("global.has", FN_GLOBAL_HAS),
    ("global.remove", FN_GLOBAL_DELETE),
];

pub fn function_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..ABI_PARAM_COUNT {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// Assinatura uniforme do __rts_dispatch: (fn_id, a0, a1, a2, a3, a4, a5) -> i64
fn dispatch_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..7 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

#[derive(Debug, Clone)]
struct LoopControlContext {
    end_label: String,
    continue_label: String,
}

fn loop_context_from_start_label(label: &str) -> Option<LoopControlContext> {
    if let Some(id) = label.strip_prefix("while_loop_") {
        return Some(LoopControlContext {
            end_label: format!("while_end_{}", id),
            continue_label: format!("while_loop_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("do_while_body_") {
        return Some(LoopControlContext {
            end_label: format!("do_while_end_{}", id),
            continue_label: format!("do_while_condition_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("for_loop_") {
        return Some(LoopControlContext {
            end_label: format!("for_end_{}", id),
            continue_label: format!("for_update_{}", id),
        });
    }
    if let Some(id) = label.strip_prefix("switch_body_") {
        // Switch só usa break — reutilizamos end_label para ambos.
        let end = format!("switch_end_{}", id);
        return Some(LoopControlContext {
            end_label: end.clone(),
            continue_label: end,
        });
    }
    None
}

fn rewrite_loop_control(instructions: &[MirInstruction]) -> Vec<MirInstruction> {
    let mut rewritten = Vec::with_capacity(instructions.len());
    let mut loop_stack: Vec<LoopControlContext> = Vec::new();

    for instruction in instructions {
        match instruction {
            MirInstruction::Label(name) => {
                if let Some(ctx) = loop_context_from_start_label(name) {
                    loop_stack.push(ctx);
                }
                rewritten.push(instruction.clone());
                if let Some(top) = loop_stack.last() {
                    if &top.end_label == name {
                        loop_stack.pop();
                    }
                }
            }
            MirInstruction::Break => {
                if let Some(top) = loop_stack.last() {
                    rewritten.push(MirInstruction::Jump(top.end_label.clone()));
                } else {
                    rewritten.push(instruction.clone());
                }
            }
            MirInstruction::Continue => {
                if let Some(top) = loop_stack.last() {
                    rewritten.push(MirInstruction::Jump(top.continue_label.clone()));
                } else {
                    rewritten.push(instruction.clone());
                }
            }
            _ => rewritten.push(instruction.clone()),
        }
    }

    rewritten
}

/// Resultado da análise de promoção de globais: nomes que a função lê/escreve
/// fora de seus próprios `Bind`s (portanto são globais) e que são seguros
/// para cachear num stack slot local durante toda a execução da função.
///
/// Segurança atual: promovemos apenas funções que **não** fazem nenhuma
/// `Call` — callees poderiam ler/escrever as mesmas globais e ver valores
/// obsoletos do namespace compartilhado. Esta análise conservadora resolve
/// o caso comum (loops aritméticos "puros") sem precisar de análise
/// interprocedural.
#[derive(Debug, Default)]
struct ShadowGlobalPlan {
    /// Nomes promovidos, em ordem determinística para emissão estável.
    names: Vec<String>,
}

fn analyze_shadow_globals(instructions: &[MirInstruction], function_name: &str) -> ShadowGlobalPlan {
    // Main nunca promove: suas vars são top-level/globais visíveis a outras funções.
    if function_name == "main" {
        return ShadowGlobalPlan::default();
    }

    let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut referenced: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut has_call = false;

    for instruction in instructions {
        match instruction {
            MirInstruction::Bind(name, _, _) => {
                // Bind marca o nome como local; a partir daqui reads do mesmo
                // nome dentro da função apontam para o binding local, não para
                // o namespace.
                locals.insert(name.clone());
            }
            MirInstruction::LoadBinding(_, name) | MirInstruction::WriteBind(name, _) => {
                referenced.insert(name.clone());
            }
            MirInstruction::Call(_, _, _) => {
                has_call = true;
            }
            _ => {}
        }
    }

    if has_call {
        // Não sabemos o que o callee faz com as globais — conservador.
        return ShadowGlobalPlan::default();
    }

    // Globais são os referenced que não viraram locals antes de serem usados.
    // BTreeSet garante ordem estável.
    let names: Vec<String> = referenced
        .into_iter()
        .filter(|name| !locals.contains(name))
        .collect();

    ShadowGlobalPlan { names }
}

pub fn define_typed_function<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    function_id: FuncId,
    function: &TypedMirFunction,
) -> Result<()> {
    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);

        let entry_params = builder.block_params(entry_block).to_vec();
        // Collect param handle info for slot creation after switch_to_block.
        let mut param_handle_entries: Vec<(usize, Value)> = Vec::new();
        for index in 0..function.param_count {
            let is_numeric = function.param_is_numeric.get(index).copied().unwrap_or(false);
            if is_numeric {
                continue;
            }
            if let Some(value) = entry_params.get(index + 1).copied() {
                param_handle_entries.push((index, value));
            }
        }
        let mut vreg_map = BTreeMap::<VReg, Value>::new();
        let mut vreg_kinds = BTreeMap::<VReg, VRegKind>::new();
        let mut const_string_vregs = BTreeMap::<VReg, String>::new();
        let mut local_bindings = BTreeMap::<String, BindingState>::new();
        // Bindings do `main` são semanticamente top-level/globais — precisam ir para o namespace
        // compartilhado para serem visíveis a outras funções. Em funções "normais", os `let`s
        // são locais e podem virar stack slots, eliminando os dispatches de Bind/Read/Write.
        let use_local_bindings = function.name != "main";

        let raw_instructions: Vec<MirInstruction> = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().cloned())
            .collect();
        let instructions = rewrite_loop_control(&raw_instructions);

        // --- Pass 1: Create Cranelift blocks for all labels ---
        let mut label_blocks = BTreeMap::<String, cranelift_codegen::ir::Block>::new();
        for instruction in &instructions {
            if let MirInstruction::Label(name) = instruction {
                if !label_blocks.contains_key(name.as_str()) {
                    let block = builder.create_block();
                    label_blocks.insert(name.clone(), block);
                }
            }
        }

        // Also create a dedicated exit block for break statements
        let exit_block = builder.create_block();

        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Store handle-typed params in stack slots for safe reload in any block.
        let mut handle_param_slots: Vec<StackSlot> = Vec::new();
        for (_index, value) in &param_handle_entries {
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            store_binding_slot(&mut builder, slot, *value);
            handle_param_slots.push(slot);
        }

        // Prologue: bind every declared user function as a NativeFunction handle so that
        // LoadBinding + FN_CALL_BY_HANDLE can call them when passed as callbacks.
        // Only emitted in `main` to avoid redundant work in other functions.
        if function.name == "main" {
            // Collect names first to avoid holding the iterator borrow alongside emit_dispatch.
            let user_fn_names: Vec<String> = func_declarations
                .keys()
                .filter(|n| !n.starts_with("__"))
                .cloned()
                .collect();
            for fn_name in user_fn_names {
                let name_data_id = declare_string_data(module, data_cache, fn_name.as_str())?;
                let name_data_ref = module.declare_data_in_func(name_data_id, builder.func);
                let name_ptr = builder.ins().symbol_value(types::I64, name_data_ref);
                let name_len = builder.ins().iconst(types::I64, fn_name.len() as i64);
                let not_mutable = builder.ins().iconst(types::I64, 0);
                // Box the function name as a NativeFunction handle
                let fn_handle = emit_dispatch(
                    module,
                    func_declarations,
                    &mut builder,
                    FN_BOX_NATIVE_FN,
                    &[name_ptr, name_len],
                )?;
                // Bind it in the VALUE_STORE so LoadBinding can find it
                emit_dispatch(
                    module,
                    func_declarations,
                    &mut builder,
                    FN_BIND_IDENTIFIER,
                    &[name_ptr, name_len, fn_handle, not_mutable],
                )?;
            }
        }

        // --- Pass 0: promoção de globais para shadow stack slots ---
        //
        // Antes de emitir o corpo, identificamos globais (nomes lidos/escritos
        // sem um Bind anterior na mesma função) que podem ser cacheadas em
        // stack slots locais pra eliminar dispatches FN_READ_IDENTIFIER /
        // FN_BIND_IDENTIFIER dentro de hot loops. Só promovemos quando a função
        // não faz nenhuma Call — callees poderiam observar valores obsoletos
        // no namespace compartilhado. Ver analyze_shadow_globals.
        //
        // Cada global promovida:
        //   1. no prólogo: READ_IDENTIFIER -> UNBOX_NUMBER -> stack slot (NativeF64)
        //   2. no corpo: Load/Write via stack slot (caminho nativo, zero dispatch)
        //   3. antes de cada return: BOX_NUMBER -> BIND_IDENTIFIER (write-back)
        let shadow_plan = if use_local_bindings {
            analyze_shadow_globals(&instructions, function.name.as_str())
        } else {
            ShadowGlobalPlan::default()
        };
        for name in &shadow_plan.names {
            let data_id = declare_string_data(module, data_cache, name.as_str())?;
            let data_ref = module.declare_data_in_func(data_id, builder.func);
            let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
            let name_len = builder.ins().iconst(types::I64, name.len() as i64);
            let handle = emit_dispatch(
                module,
                func_declarations,
                &mut builder,
                FN_READ_IDENTIFIER,
                &[name_ptr, name_len],
            )?;
            // Unbox para F64 bits. Se a global não for número, FN_UNBOX_NUMBER
            // retorna NaN via to_number() — comportamento compatível com JS
            // semântico de `Number(valor)`.
            let bits = emit_dispatch(
                module,
                func_declarations,
                &mut builder,
                FN_UNBOX_NUMBER,
                &[handle],
            )?;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            store_binding_slot(&mut builder, slot, bits);
            local_bindings.insert(
                name.clone(),
                BindingState {
                    slot,
                    mutable: true,
                    kind: VRegKind::NativeF64,
                },
            );
        }

        let mut default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        let mut default_return_is_native = false;
        let mut terminated = false;

        // --- Pass 2: Emit instructions with real control flow ---
        for instruction in &instructions {
            if terminated {
                // If we hit a label after termination, switch to that block
                if let MirInstruction::Label(name) = instruction {
                    if let Some(&target_block) = label_blocks.get(name.as_str()) {
                        builder.switch_to_block(target_block);
                        // Reset default_return ao trocar de bloco: o valor
                        // anterior pode ter sido produzido em um bloco que
                        // nao domina o bloco atual (caso classico em
                        // if/else), quebrando o SSA do Cranelift. Ao emitir
                        // um novo iconst no bloco corrente, garantimos
                        // dominancia do return implícito no final.
                        default_return = builder
                            .ins()
                            .iconst(types::I64, ABI_UNDEFINED_HANDLE);
                        default_return_is_native = false;
                        terminated = false;
                    }
                }
                if terminated {
                    continue;
                }
                // Re-check current instruction (Label was handled above, skip it)
                if matches!(instruction, MirInstruction::Label(_)) {
                    continue;
                }
            }

            match instruction {
                MirInstruction::ConstNumber(dst, val) => {
                    let bits = i64::from_ne_bytes(val.to_ne_bytes());
                    let result = builder.ins().iconst(types::I64, bits);
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeF64);
                    default_return = result;
                    default_return_is_native = true;
                }

                MirInstruction::ConstInt32(dst, val) => {
                    let result = builder.ins().iconst(types::I64, *val as i64);
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeI32);
                    default_return = result;
                    default_return_is_native = true;
                }

                MirInstruction::ConstString(dst, s) => {
                    let result = emit_box_string(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        s.as_str(),
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    const_string_vregs.insert(*dst, s.clone());
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstBool(dst, b) => {
                    // Emit como handle de RuntimeValue::Bool via FN_BOX_BOOL.
                    //
                    // Antigamente esta variante emitia NativeI32 (0/1) direto e
                    // confiava em ensure_handle para boxar quando preciso. O
                    // problema: `ensure_handle` chama box_native_i32 que
                    // converte para f64 e cria RuntimeValue::Number, perdendo
                    // a informacao de bool. Resultado pratico: JSON.stringify
                    // de um campo bool serializava "true" como 1 ("flag":1).
                    //
                    // Correcao: emit direto como handle. Perdemos o fast path
                    // local (comparacoes e if/while usam outros caminhos que
                    // nao dependem disto), mas ganhamos tipo correto em boxing.
                    let flag = builder.ins().iconst(types::I64, if *b { 1 } else { 0 });
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BOX_BOOL,
                        &[flag],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstNull(dst) => {
                    // null maps to UNDEFINED_HANDLE (0); indistinguishable at this level.
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstUndef(dst) => {
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
                }

                MirInstruction::LoadParam(dst, index) => {
                    let handle = entry_params
                        .get(index + 1)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                    // Parâmetros numéricos (anotação `number`/`i32`/etc. no HIR)
                    // são unboxed UMA VEZ aqui no entry block para evitar
                    // FN_UNBOX_NUMBER em cada uso dentro de loops. Parâmetros
                    // sem anotação ou com tipo não-numérico permanecem como
                    // handles — o `adapt_to_kind` genérico do BinOp faz a
                    // conversão caso-a-caso quando necessário, e callees como
                    // `io.print(msg: str)` recebem o handle direto.
                    if function.param_is_numeric.get(*index).copied().unwrap_or(false) {
                        let bits = emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            FN_UNBOX_NUMBER,
                            &[handle],
                        )?;
                        vreg_map.insert(*dst, bits);
                        vreg_kinds.insert(*dst, VRegKind::NativeF64);
                    } else {
                        vreg_map.insert(*dst, handle);
                        // VRegKind default é Handle.
                    }
                }

                MirInstruction::BinOp(dst, op, lhs, rhs) => {
                    let mut lhs_kind = vreg_kinds.get(lhs).copied().unwrap_or(VRegKind::Handle);
                    let mut rhs_kind = vreg_kinds.get(rhs).copied().unwrap_or(VRegKind::Handle);
                    let is_arith = matches!(
                        op,
                        MirBinOp::Add
                            | MirBinOp::Sub
                            | MirBinOp::Mul
                            | MirBinOp::Div
                            | MirBinOp::Mod
                    );
                    let is_cmp = matches!(
                        op,
                        MirBinOp::Lt
                            | MirBinOp::Lte
                            | MirBinOp::Gt
                            | MirBinOp::Gte
                            | MirBinOp::Eq
                            | MirBinOp::Ne
                    );

                    // Resolve os valores uma vez e aplica promoção numérica quando
                    // os kinds divergem. Unificamos no kind mais largo (Handle/F64 > I32)
                    // antes das branches nativas.
                    //
                    // Caso especial: `Add` com pelo menos um Handle é ambíguo —
                    // pode ser concat de string ou soma numérica. O runtime
                    // decide em FN_BINOP via `is_string_like`. Por isso a
                    // promoção Handle↔Native é DESLIGADA para Add: ambos são
                    // mantidos como Handle (boxando o native se necessário) e
                    // o fallback dispatch cuida do resto.
                    let mut lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                    let mut rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                    let has_handle_operand =
                        lhs_kind == VRegKind::Handle || rhs_kind == VRegKind::Handle;
                    let skip_numeric_promotion =
                        matches!(op, MirBinOp::Add) && has_handle_operand;
                    if (is_arith || is_cmp)
                        && lhs_kind != rhs_kind
                        && !skip_numeric_promotion
                    {
                        let target = match (lhs_kind, rhs_kind) {
                            (VRegKind::NativeI32, VRegKind::NativeF64)
                            | (VRegKind::NativeF64, VRegKind::NativeI32)
                            | (VRegKind::Handle, VRegKind::NativeF64)
                            | (VRegKind::NativeF64, VRegKind::Handle)
                            | (VRegKind::Handle, VRegKind::NativeI32)
                            | (VRegKind::NativeI32, VRegKind::Handle) => VRegKind::NativeF64,
                            _ => VRegKind::Handle,
                        };
                        if target != VRegKind::Handle {
                            if lhs_kind != target {
                                lhs_val = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    lhs_val,
                                    lhs_kind,
                                    target,
                                )?;
                                lhs_kind = target;
                            }
                            if rhs_kind != target {
                                rhs_val = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    rhs_val,
                                    rhs_kind,
                                    target,
                                )?;
                                rhs_kind = target;
                            }
                        }
                    }

                    if lhs_kind == VRegKind::NativeI32 && rhs_kind == VRegKind::NativeI32 && is_cmp
                    {
                        // Native i32 comparison — returns 0 or 1 as i64
                        let lhs_i32 = builder.ins().ireduce(types::I32, lhs_val);
                        let rhs_i32 = builder.ins().ireduce(types::I32, rhs_val);
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let cc = match op {
                            MirBinOp::Lt => IntCC::SignedLessThan,
                            MirBinOp::Lte => IntCC::SignedLessThanOrEqual,
                            MirBinOp::Gt => IntCC::SignedGreaterThan,
                            MirBinOp::Gte => IntCC::SignedGreaterThanOrEqual,
                            MirBinOp::Eq => IntCC::Equal,
                            MirBinOp::Ne => IntCC::NotEqual,
                            _ => unreachable!(),
                        };
                        let cmp = builder.ins().icmp(cc, lhs_i32, rhs_i32);
                        let result = builder.ins().uextend(types::I64, cmp);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeF64
                        && rhs_kind == VRegKind::NativeF64
                        && is_cmp
                    {
                        // Native f64 comparison — returns 0 or 1 as i64
                        let lhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), lhs_val);
                        let rhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), rhs_val);
                        use cranelift_codegen::ir::condcodes::FloatCC;
                        let cc = match op {
                            MirBinOp::Lt => FloatCC::LessThan,
                            MirBinOp::Lte => FloatCC::LessThanOrEqual,
                            MirBinOp::Gt => FloatCC::GreaterThan,
                            MirBinOp::Gte => FloatCC::GreaterThanOrEqual,
                            MirBinOp::Eq => FloatCC::Equal,
                            MirBinOp::Ne => FloatCC::NotEqual,
                            _ => unreachable!(),
                        };
                        let cmp = builder.ins().fcmp(cc, lhs_f64, rhs_f64);
                        let result = builder.ins().uextend(types::I64, cmp);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeI32
                        && rhs_kind == VRegKind::NativeI32
                        && is_arith
                    {
                        // Native i32 arithmetic path
                        let lhs_i32 = builder.ins().ireduce(types::I32, lhs_val);
                        let rhs_i32 = builder.ins().ireduce(types::I32, rhs_val);
                        let result_i32 = match op {
                            MirBinOp::Add => builder.ins().iadd(lhs_i32, rhs_i32),
                            MirBinOp::Sub => builder.ins().isub(lhs_i32, rhs_i32),
                            MirBinOp::Mul => builder.ins().imul(lhs_i32, rhs_i32),
                            MirBinOp::Div => builder.ins().sdiv(lhs_i32, rhs_i32),
                            MirBinOp::Mod => builder.ins().srem(lhs_i32, rhs_i32),
                            _ => unreachable!(),
                        };
                        let result = builder.ins().sextend(types::I64, result_i32);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeF64
                        && rhs_kind == VRegKind::NativeF64
                        && is_arith
                    {
                        // Native f64 arithmetic path
                        let lhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), lhs_val);
                        let rhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), rhs_val);
                        let result_f64 = match op {
                            MirBinOp::Add => builder.ins().fadd(lhs_f64, rhs_f64),
                            MirBinOp::Sub => builder.ins().fsub(lhs_f64, rhs_f64),
                            MirBinOp::Mul => builder.ins().fmul(lhs_f64, rhs_f64),
                            MirBinOp::Div => builder.ins().fdiv(lhs_f64, rhs_f64),
                            MirBinOp::Mod => {
                                // JS remainder semantics: a - trunc(a / b) * b
                                let div = builder.ins().fdiv(lhs_f64, rhs_f64);
                                let truncated = builder.ins().trunc(div);
                                let product = builder.ins().fmul(truncated, rhs_f64);
                                builder.ins().fsub(lhs_f64, product)
                            }
                            _ => unreachable!(),
                        };
                        let result = builder
                            .ins()
                            .bitcast(types::I64, MemFlags::new(), result_f64);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeF64);
                        default_return = result;
                        default_return_is_native = true;
                    } else {
                        // Fallback: box any native operands, então chama __rts_dispatch(FN_BINOP, op, lhs, rhs, 0, 0, 0)
                        let lhs_val = ensure_handle(
                            &vreg_map,
                            &vreg_kinds,
                            lhs,
                            module,
                            func_declarations,
                            &mut builder,
                        )?;
                        let rhs_val = ensure_handle(
                            &vreg_map,
                            &vreg_kinds,
                            rhs,
                            module,
                            func_declarations,
                            &mut builder,
                        )?;
                        let op_tag = binop_to_tag(op);
                        let op_val = builder.ins().iconst(types::I64, op_tag);
                        let result = emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            FN_BINOP,
                            &[op_val, lhs_val, rhs_val],
                        )?;
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::Handle);
                        default_return = result;
                        default_return_is_native = false;
                    }
                }

                MirInstruction::UnaryOp(dst, op, src) => {
                    let src_kind = vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                    let src_val = resolve_vreg(&vreg_map, src, &mut builder);
                    let (result, result_kind) = match op {
                        MirUnaryOp::Negate if src_kind == VRegKind::NativeI32 => {
                            // Native i32 negate
                            let src_i32 = builder.ins().ireduce(types::I32, src_val);
                            let neg = builder.ins().ineg(src_i32);
                            let r = builder.ins().sextend(types::I64, neg);
                            (r, VRegKind::NativeI32)
                        }
                        MirUnaryOp::Negate if src_kind == VRegKind::NativeF64 => {
                            // Native fneg
                            let src_f64 =
                                builder.ins().bitcast(types::F64, MemFlags::new(), src_val);
                            let neg = builder.ins().fneg(src_f64);
                            let r = builder.ins().bitcast(types::I64, MemFlags::new(), neg);
                            (r, VRegKind::NativeF64)
                        }
                        MirUnaryOp::Negate => {
                            // Fallback: -x == 0 - x via runtime
                            let zero_bits = i64::from_ne_bytes(0.0f64.to_ne_bytes());
                            let zero_raw = builder.ins().iconst(types::I64, zero_bits);
                            let zero_handle = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BOX_NUMBER,
                                &[zero_raw],
                            )?;
                            let op_val = builder
                                .ins()
                                .iconst(types::I64, binop_to_tag(&MirBinOp::Sub));
                            let result = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BINOP,
                                &[op_val, zero_handle, src_val],
                            )?;
                            (result, VRegKind::Handle)
                        }
                        MirUnaryOp::Not => {
                            // !x: box native numbers first if needed
                            let handle_val = match src_kind {
                                VRegKind::NativeF64 => box_native_f64(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                )?,
                                VRegKind::NativeI32 => box_native_i32(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                )?,
                                _ => src_val,
                            };
                            let false_i32 = builder.ins().iconst(types::I64, 0);
                            let false_handle =
                                box_native_i32(module, func_declarations, &mut builder, false_i32)?;
                            let op_val = builder
                                .ins()
                                .iconst(types::I64, binop_to_tag(&MirBinOp::Eq));
                            let result = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BINOP,
                                &[op_val, handle_val, false_handle],
                            )?;
                            (result, VRegKind::Handle)
                        }
                        MirUnaryOp::Positive => {
                            // +x is identity for numbers
                            (src_val, src_kind)
                        }
                    };
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, result_kind);
                    default_return = result;
                    default_return_is_native =
                        matches!(result_kind, VRegKind::NativeF64 | VRegKind::NativeI32);
                }

                MirInstruction::Call(dst, callee, args) => {
                    // Refresh const string vregs to ensure dominance in the
                    // current block. Without this, a ConstString boxed in a
                    // prior block (e.g. loop body) would produce a Value that
                    // doesn't dominate the current insertion point.
                    for (vreg, text) in const_string_vregs
                        .iter()
                        .map(|(v, s)| (*v, s.clone()))
                        .collect::<Vec<_>>()
                    {
                        if !vreg_map.contains_key(&vreg) {
                            continue;
                        }
                        let refreshed = emit_box_string(
                            module,
                            func_declarations,
                            data_cache,
                            &mut builder,
                            text.as_str(),
                        )?;
                        vreg_map.insert(vreg, refreshed);
                    }

                    let result = if func_declarations.contains_key(callee.as_str()) {
                        // Direct call to a known user function
                        let callee_id = func_declarations[callee.as_str()];
                        let mut call_args = Vec::with_capacity(ABI_PARAM_COUNT);
                        call_args.push(builder.ins().iconst(types::I64, args.len() as i64));
                        for arg in args.iter().take(ABI_ARG_SLOTS) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            call_args.push(val);
                        }
                        while call_args.len() < ABI_PARAM_COUNT {
                            call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                        }
                        let local = module.declare_func_in_func(callee_id, builder.func);
                        let call = builder.ins().call(local, &call_args);
                        builder.inst_results(call)[0]
                    } else if let Some(&(_, fn_id_val)) = CALLEE_FN_IDS
                        .iter()
                        .find(|(name, _)| *name == callee.as_str())
                    {
                        // Known namespace callee — emite __rts_dispatch(fn_id, args...) diretamente.
                        let mut handle_args: Vec<Value> = Vec::with_capacity(args.len());
                        for arg in args.iter().take(6) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            handle_args.push(val);
                        }
                        emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            fn_id_val,
                            &handle_args,
                        )?
                    } else if crate::namespaces::is_catalog_callee(callee.as_str()) {
                        // Dynamic fallback: namespace callee not in CALLEE_FN_IDS.
                        // Route through __rts_call_dispatch so all registered namespaces work.
                        let mut handle_args: Vec<Value> = Vec::with_capacity(args.len());
                        for arg in args.iter().take(6) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            handle_args.push(val);
                        }

                        let pinned_values = pin_live_handles_for_dynamic_call(
                            module,
                            func_declarations,
                            &mut builder,
                            &local_bindings,
                            &handle_args,
                            &[],
                            &handle_param_slots,
                        )?;
                        let result = emit_call_dispatch(
                            module,
                            func_declarations,
                            data_cache,
                            &mut builder,
                            callee.as_str(),
                            &handle_args,
                        )?;
                        unpin_live_handles_after_dynamic_call(
                            module,
                            func_declarations,
                            &mut builder,
                            &pinned_values,
                        )?;
                        result
                    } else {
                        // Last resort: callee may be a local binding holding a NativeFunction handle.
                        // Emit FN_CALL_BY_HANDLE(fn_handle, argc=0) so the runtime can look up
                        // and call it (e.g., a named user function passed as a callback).
                        let fn_handle = if use_local_bindings {
                            if let Some(state) = local_bindings.get(callee.as_str()) {
                                builder.ins().stack_load(types::I64, state.slot, 0)
                            } else {
                                // Try from global VALUE_STORE
                                let data_id = declare_string_data(module, data_cache, callee.as_str())?;
                                let data_ref = module.declare_data_in_func(data_id, builder.func);
                                let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                                let name_len = builder.ins().iconst(types::I64, callee.len() as i64);
                                emit_dispatch(module, func_declarations, &mut builder, FN_READ_IDENTIFIER, &[name_ptr, name_len])?
                            }
                        } else {
                            let data_id = declare_string_data(module, data_cache, callee.as_str())?;
                            let data_ref = module.declare_data_in_func(data_id, builder.func);
                            let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                            let name_len = builder.ins().iconst(types::I64, callee.len() as i64);
                            emit_dispatch(module, func_declarations, &mut builder, FN_READ_IDENTIFIER, &[name_ptr, name_len])?
                        };
                        let argc = builder.ins().iconst(types::I64, args.len() as i64);
                        emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            FN_CALL_BY_HANDLE,
                            &[fn_handle, argc],
                        )?
                    };
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::Bind(name, src, mutable) => {
                    if use_local_bindings {
                        let src_kind = vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                        let src_val = resolve_vreg(&vreg_map, src, &mut builder);

                        if let Some(state) = local_bindings.get_mut(name) {
                            // Re-binding de uma variável já existente: adapta o novo valor ao
                            // kind fixo do slot.
                            let adapted = adapt_to_kind(
                                module,
                                func_declarations,
                                &mut builder,
                                src_val,
                                src_kind,
                                state.kind,
                            )?;
                            store_binding_slot(&mut builder, state.slot, adapted);
                            state.mutable = *mutable;
                            continue;
                        }

                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        // O primeiro Bind fixa o kind do slot — escolhemos o kind do src
                        // para manter o caminho nativo quando possível.
                        store_binding_slot(&mut builder, slot, src_val);
                        local_bindings.insert(
                            name.clone(),
                            BindingState {
                                slot,
                                mutable: *mutable,
                                kind: src_kind,
                            },
                        );
                        continue;
                    }

                    let value_handle = ensure_handle(
                        &vreg_map,
                        &vreg_kinds,
                        src,
                        module,
                        func_declarations,
                        &mut builder,
                    )?;
                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder
                        .ins()
                        .iconst(types::I64, if *mutable { 1 } else { 0 });
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BIND_IDENTIFIER,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    )?;
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::WriteBind(name, src) => {
                    if use_local_bindings {
                        if let Some(state) = local_bindings.get(name).copied() {
                            if state.mutable {
                                let src_kind =
                                    vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                                let src_val = resolve_vreg(&vreg_map, src, &mut builder);
                                let adapted = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                    src_kind,
                                    state.kind,
                                )?;
                                store_binding_slot(&mut builder, state.slot, adapted);
                                continue;
                            }
                        }
                    }

                    // Fallback para bindings não locais.
                    let value_handle = ensure_handle(
                        &vreg_map,
                        &vreg_kinds,
                        src,
                        module,
                        func_declarations,
                        &mut builder,
                    )?;
                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder.ins().iconst(types::I64, 1i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BIND_IDENTIFIER,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    )?;
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::LoadBinding(dst, name) => {
                    if use_local_bindings {
                        if let Some(state) = local_bindings.get(name) {
                            let result = load_binding_slot(&mut builder, state.slot);
                            vreg_map.insert(*dst, result);
                            vreg_kinds.insert(*dst, state.kind);
                            default_return = result;
                            default_return_is_native = matches!(
                                state.kind,
                                VRegKind::NativeF64 | VRegKind::NativeI32
                            );
                            continue;
                        }
                    }

                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_READ_IDENTIFIER,
                        &[name_ptr, name_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::Return(Some(vreg)) => {
                    let raw = resolve_vreg(&vreg_map, vreg, &mut builder);
                    let value = match vreg_kinds.get(vreg) {
                        Some(&VRegKind::NativeF64) => {
                            box_native_f64(module, func_declarations, &mut builder, raw)?
                        }
                        Some(&VRegKind::NativeI32) => {
                            box_native_i32(module, func_declarations, &mut builder, raw)?
                        }
                        _ => raw,
                    };
                    emit_shadow_writeback(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        &shadow_plan.names,
                        &local_bindings,
                    )?;
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Return(None) => {
                    let value = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    emit_shadow_writeback(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        &shadow_plan.names,
                        &local_bindings,
                    )?;
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Import { .. } => {
                    // No-op: imports are resolved at link time
                }

                MirInstruction::Jump(label) => {
                    if !terminated {
                        if let Some(&target_block) = label_blocks.get(label.as_str()) {
                            // At loop back-edges (jump to while_loop_*), emit a
                            // compact pass. At this point all temporaries from the
                            // iteration are dead — only bindings and pinned handles
                            // survive. This keeps the ValueStore bounded in loops.
                            if label.starts_with("while_loop_") || label.starts_with("do_while_") {
                                let undefined = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                                let _ = emit_dispatch(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    FN_COMPACT_EXCLUDING,
                                    &[undefined],
                                )?;
                            }
                            builder.ins().jump(target_block, &[]);
                            terminated = true;
                        }
                    }
                }

                MirInstruction::JumpIf(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds
                            .get(condition)
                            .copied()
                            .unwrap_or(VRegKind::Handle);
                        // Para handles, chama __rts_dispatch(FN_IS_TRUTHY, handle, ...) para obter 0/1
                        let bool_val = if cond_kind == VRegKind::Handle {
                            emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_IS_TRUTHY,
                                &[cond_val],
                            )?
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            bool_val,
                            zero,
                        );
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::JumpIfNot(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds
                            .get(condition)
                            .copied()
                            .unwrap_or(VRegKind::Handle);
                        let bool_val = if cond_kind == VRegKind::Handle {
                            emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_IS_TRUTHY,
                                &[cond_val],
                            )?
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            bool_val,
                            zero,
                        );
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::Label(label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        // Fall through from current block to label block
                        builder.ins().jump(target_block, &[]);
                        builder.switch_to_block(target_block);
                        // Ver comentario na transicao similar em Label
                        // apos terminated=true. O default_return precisa
                        // ser re-emitido no bloco novo para preservar
                        // dominancia do return implícito.
                        default_return = builder
                            .ins()
                            .iconst(types::I64, ABI_UNDEFINED_HANDLE);
                        default_return_is_native = false;
                    }
                }

                MirInstruction::Break => {
                    builder.ins().jump(exit_block, &[]);
                    terminated = true;
                }

                MirInstruction::Continue => {
                    // Continue jumps back to the nearest loop header
                    // For now, treat as no-op (requires loop tracking)
                }

                MirInstruction::RuntimeEval(dst, text) => {
                    let data_id = declare_string_data(module, data_cache, text.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let text_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let text_len = builder.ins().iconst(types::I64, text.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_EVAL_STMT,
                        &[text_ptr, text_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::NewInstance(dst, class_name) => {
                    // Aloca RuntimeValue::Object vazio via FN_NEW_INSTANCE.
                    // O class_name é passado por enquanto apenas como diagnóstico
                    // — o runtime ainda ignora (ver abi.rs FN_NEW_INSTANCE).
                    let data_id =
                        declare_string_data(module, data_cache, class_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len =
                        builder.ins().iconst(types::I64, class_name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_NEW_INSTANCE,
                        &[name_ptr, name_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::LoadField(dst, obj_vreg, field_name) => {
                    // dst = obj.field
                    let obj_value = resolve_vreg(&vreg_map, obj_vreg, &mut builder);
                    let obj_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        obj_value,
                        vreg_kinds
                            .get(obj_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let data_id =
                        declare_string_data(module, data_cache, field_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let field_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let field_len =
                        builder.ins().iconst(types::I64, field_name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_LOAD_FIELD,
                        &[obj_handle, field_ptr, field_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::StoreField(obj_vreg, field_name, value_vreg) => {
                    // obj.field = value
                    let obj_value = resolve_vreg(&vreg_map, obj_vreg, &mut builder);
                    let obj_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        obj_value,
                        vreg_kinds
                            .get(obj_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let value_raw = resolve_vreg(&vreg_map, value_vreg, &mut builder);
                    let value_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        value_raw,
                        vreg_kinds
                            .get(value_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let data_id =
                        declare_string_data(module, data_cache, field_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let field_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let field_len =
                        builder.ins().iconst(types::I64, field_name.len() as i64);
                    let _ = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_STORE_FIELD,
                        &[obj_handle, field_ptr, field_len, value_handle],
                    )?;
                    // StoreField não produz valor consumível — não muda
                    // default_return.
                }
            }
        }

        if !terminated {
            let ret_val = if default_return_is_native {
                box_native_f64(module, func_declarations, &mut builder, default_return)?
            } else {
                default_return
            };
            emit_shadow_writeback(
                module,
                func_declarations,
                data_cache,
                &mut builder,
                &shadow_plan.names,
                &local_bindings,
            )?;
            builder.ins().return_(&[ret_val]);
        }

        // Seal and finalize the exit block (used by Break)
        builder.switch_to_block(exit_block);
        let exit_ret = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        emit_shadow_writeback(
            module,
            func_declarations,
            data_cache,
            &mut builder,
            &shadow_plan.names,
            &local_bindings,
        )?;
        builder.ins().return_(&[exit_ret]);

        // Let Cranelift resolve remaining block seals once CFG is complete.
        builder.seal_all_blocks();
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| format!("failed to define typed function '{}'", function.name))?;
    module.clear_context(&mut context);
    Ok(())
}

fn resolve_vreg(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg: &VReg,
    builder: &mut FunctionBuilder,
) -> Value {
    vreg_map
        .get(vreg)
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE))
}

fn store_binding_slot(builder: &mut FunctionBuilder, slot: StackSlot, value: Value) {
    let addr = builder.ins().stack_addr(types::I64, slot, 0);
    builder.ins().store(MemFlags::new(), value, addr, 0);
}

/// Emite o write-back dos shadow globals para o namespace compartilhado.
/// Chamado antes de cada `return_` do caminho principal, garantindo que
/// qualquer escrita local à global seja visível a callees subsequentes.
/// Globais promovidas são sempre NativeF64; box + bind para Handle.
fn emit_shadow_writeback<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    shadow_names: &[String],
    local_bindings: &BTreeMap<String, BindingState>,
) -> Result<()> {
    for name in shadow_names {
        let Some(state) = local_bindings.get(name) else {
            continue;
        };
        // Lê o valor atual do slot (bits F64).
        let bits = load_binding_slot(builder, state.slot);
        // Box: f64 bits -> handle no namespace.
        let handle = emit_dispatch(
            module,
            func_declarations,
            builder,
            FN_BOX_NUMBER,
            &[bits],
        )?;
        // Bind para o namespace: reutiliza o nome já presente no data segment.
        let data_id = declare_string_data(module, data_cache, name.as_str())?;
        let data_ref = module.declare_data_in_func(data_id, builder.func);
        let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
        let name_len = builder.ins().iconst(types::I64, name.len() as i64);
        let mutable_flag = builder.ins().iconst(types::I64, 1);
        emit_dispatch(
            module,
            func_declarations,
            builder,
            FN_BIND_IDENTIFIER,
            &[name_ptr, name_len, handle, mutable_flag],
        )?;
    }
    Ok(())
}

fn load_binding_slot(builder: &mut FunctionBuilder, slot: StackSlot) -> Value {
    let addr = builder.ins().stack_addr(types::I64, slot, 0);
    builder.ins().load(types::I64, MemFlags::new(), addr, 0)
}

fn pin_live_handles_for_dynamic_call<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    local_bindings: &BTreeMap<String, BindingState>,
    call_args: &[Value],
    extra_handles: &[Value],
    param_handle_slots: &[StackSlot],
) -> Result<Vec<Value>> {
    let mut pinned_values = Vec::new();

    for state in local_bindings.values() {
        if state.kind == VRegKind::Handle {
            let handle = load_binding_slot(builder, state.slot);
            let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
            pinned_values.push(handle);
        }
    }

    for &arg in call_args {
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[arg])?;
        pinned_values.push(arg);
    }

    for &handle in extra_handles {
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
        pinned_values.push(handle);
    }

    // Reload param handles from stack slots — safe in any block since
    // stack_load always dominates the current insertion point.
    for &slot in param_handle_slots {
        let handle = load_binding_slot(builder, slot);
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
        pinned_values.push(handle);
    }

    Ok(pinned_values)
}

fn unpin_live_handles_after_dynamic_call<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    pinned_values: &[Value],
) -> Result<()> {
    for &handle in pinned_values {
        let _ = emit_dispatch(module, func_declarations, builder, FN_UNPIN_HANDLE, &[handle])?;
    }
    Ok(())
}

fn ensure_import<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    name: &str,
    signature: &cranelift_codegen::ir::Signature,
) -> Result<FuncId> {
    if let Some(id) = declarations.get(name).copied() {
        return Ok(id);
    }
    let id = module
        .declare_function(name, Linkage::Import, signature)
        .with_context(|| format!("failed to declare imported helper '{}'", name))?;
    declarations.insert(name.to_string(), id);
    Ok(id)
}

fn declare_string_data<M: Module>(
    module: &mut M,
    data_cache: &mut BTreeMap<String, DataId>,
    text: &str,
) -> Result<DataId> {
    if let Some(id) = data_cache.get(text).copied() {
        return Ok(id);
    }
    let symbol = format!("__rts_typed_{:016x}", stable_hash(text));
    let id = module
        .declare_data(&symbol, Linkage::Local, false, false)
        .with_context(|| format!("failed to declare typed data symbol '{}'", symbol))?;
    let mut desc = DataDescription::new();
    desc.define(text.as_bytes().to_vec().into_boxed_slice());
    module
        .define_data(id, &desc)
        .with_context(|| format!("failed to define typed data payload for '{}'", symbol))?;
    data_cache.insert(text.to_string(), id);
    Ok(id)
}

fn stable_hash(input: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

/// Emite uma chamada a __rts_dispatch(fn_id, args...) preenchendo com UNDEFINED_HANDLE até 6 slots.
fn emit_dispatch<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    fn_id: i64,
    args: &[Value],
) -> Result<Value> {
    let sig = dispatch_signature(module);
    let dispatch_fn = ensure_import(module, declarations, RTS_DISPATCH, &sig)?;
    let mut call_args: Vec<Value> = Vec::with_capacity(7);
    call_args.push(builder.ins().iconst(types::I64, fn_id));
    for &arg in args.iter().take(6) {
        call_args.push(arg);
    }
    while call_args.len() < 7 {
        call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
    }
    let local = module.declare_func_in_func(dispatch_fn, builder.func);
    let call = builder.ins().call(local, &call_args);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

/// Assinatura do __rts_call_dispatch: (callee_ptr, callee_len, argc, a0..a5) -> i64
fn call_dispatch_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..9 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// Emite uma chamada a __rts_call_dispatch com o callee como string estática e os args como handles.
fn emit_call_dispatch<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    callee: &str,
    args: &[Value],
) -> Result<Value> {
    use crate::codegen::cranelift::mir_parse::RTS_CALL_DISPATCH_SYMBOL;
    let sig = call_dispatch_signature(module);
    let fn_id = ensure_import(module, declarations, RTS_CALL_DISPATCH_SYMBOL, &sig)?;

    let data_id = declare_string_data(module, data_cache, callee)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, callee.len() as i64);
    let argc = builder.ins().iconst(types::I64, args.len() as i64);

    let mut call_args: Vec<Value> = Vec::with_capacity(9);
    call_args.push(ptr);
    call_args.push(len);
    call_args.push(argc);
    for &arg in args.iter().take(6) {
        call_args.push(arg);
    }
    while call_args.len() < 9 {
        call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
    }

    let local = module.declare_func_in_func(fn_id, builder.func);
    let call = builder.ins().call(local, &call_args);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

fn emit_box_string<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    text: &str,
) -> Result<Value> {
    let data_id = declare_string_data(module, data_cache, text)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, text.len() as i64);
    emit_dispatch(module, declarations, builder, FN_BOX_STRING, &[ptr, len])
}

/// Converte `value` do kind `from` para o kind `to`, emitindo conversões nativas
/// onde possível e caindo em dispatch apenas como último recurso.
fn adapt_to_kind<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    value: Value,
    from: VRegKind,
    to: VRegKind,
) -> Result<Value> {
    if from == to {
        return Ok(value);
    }
    match (from, to) {
        // NativeI32 -> NativeF64: sextend + convert
        (VRegKind::NativeI32, VRegKind::NativeF64) => {
            let i32_val = builder.ins().ireduce(types::I32, value);
            let f64_val = builder.ins().fcvt_from_sint(types::F64, i32_val);
            Ok(builder.ins().bitcast(types::I64, MemFlags::new(), f64_val))
        }
        // NativeF64 -> NativeI32: truncate
        (VRegKind::NativeF64, VRegKind::NativeI32) => {
            let f64_val = builder.ins().bitcast(types::F64, MemFlags::new(), value);
            let i32_val = builder.ins().fcvt_to_sint(types::I32, f64_val);
            Ok(builder.ins().sextend(types::I64, i32_val))
        }
        // Handle -> NativeF64: unbox via dispatch
        (VRegKind::Handle, VRegKind::NativeF64) => {
            let bits = emit_dispatch(
                module,
                func_declarations,
                builder,
                crate::namespaces::abi::FN_UNBOX_NUMBER,
                &[value],
            )?;
            Ok(bits)
        }
        // Handle -> NativeI32: unbox via dispatch + truncate
        (VRegKind::Handle, VRegKind::NativeI32) => {
            let bits = emit_dispatch(
                module,
                func_declarations,
                builder,
                crate::namespaces::abi::FN_UNBOX_NUMBER,
                &[value],
            )?;
            let f64_val = builder.ins().bitcast(types::F64, MemFlags::new(), bits);
            let i32_val = builder.ins().fcvt_to_sint(types::I32, f64_val);
            Ok(builder.ins().sextend(types::I64, i32_val))
        }
        // NativeF64 -> Handle: box
        (VRegKind::NativeF64, VRegKind::Handle) => {
            box_native_f64(module, func_declarations, builder, value)
        }
        // NativeI32 -> Handle: box
        (VRegKind::NativeI32, VRegKind::Handle) => {
            box_native_i32(module, func_declarations, builder, value)
        }
        // Mesmos kinds são tratados no early-return acima; esta arm é só para exaustividade.
        (VRegKind::Handle, VRegKind::Handle)
        | (VRegKind::NativeF64, VRegKind::NativeF64)
        | (VRegKind::NativeI32, VRegKind::NativeI32) => Ok(value),
    }
}

fn ensure_handle<M: Module>(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg_kinds: &BTreeMap<VReg, VRegKind>,
    vreg: &VReg,
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
) -> Result<Value> {
    let val = resolve_vreg(vreg_map, vreg, builder);
    match vreg_kinds.get(vreg) {
        Some(&VRegKind::NativeF64) => box_native_f64(module, func_declarations, builder, val),
        Some(&VRegKind::NativeI32) => box_native_i32(module, func_declarations, builder, val),
        _ => Ok(val),
    }
}

fn box_native_f64<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    bits: Value,
) -> Result<Value> {
    emit_dispatch(module, func_declarations, builder, FN_BOX_NUMBER, &[bits])
}

fn box_native_i32<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    i32_val: Value,
) -> Result<Value> {
    let i32_reduced = builder.ins().ireduce(types::I32, i32_val);
    let f64_val = builder.ins().fcvt_from_sint(types::F64, i32_reduced);
    let f64_bits = builder.ins().bitcast(types::I64, MemFlags::new(), f64_val);
    emit_dispatch(
        module,
        func_declarations,
        builder,
        FN_BOX_NUMBER,
        &[f64_bits],
    )
}

fn binop_to_tag(op: &MirBinOp) -> i64 {
    match op {
        MirBinOp::Add => 0,
        MirBinOp::Sub => 1,
        MirBinOp::Mul => 2,
        MirBinOp::Div => 3,
        MirBinOp::Mod => 4,
        MirBinOp::Gt => 5,
        MirBinOp::Gte => 6,
        MirBinOp::Lt => 7,
        MirBinOp::Lte => 8,
        MirBinOp::Eq => 9,
        MirBinOp::Ne => 10,
        MirBinOp::LogicAnd => 11,
        MirBinOp::LogicOr => 12,
    }
}
