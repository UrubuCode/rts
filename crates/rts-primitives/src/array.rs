//! Spec da classe primordial `Array` (metadata pura — fn_ptr null, símbolos
//! `__RTS_FN_NS_COLLECTIONS_VEC_*`). Os corpos `#[no_mangle]` ficam em
//! `rts-shared/collections/vec.rs`; aqui só a declaração de membros que o motor
//! resolve via Registry. Movida de `rts-shared/collections` na Fase 2 (Array é
//! primordial). Crate só depende de `rts-engine`.

/// Registra a classe global `Array` (receiver = handle de Vec). Só os métodos
/// LIMPOS recv-only (sem callback/variádico/arg-overload): pop/shift (retorno
/// ambíguo — elemento ou undefined), reverse/toReversed/values/keys/entries.
/// O codegen resolve `arr.method()` pelo Registry, sem braço hardcoded; métodos
/// ausentes (push/sort/map/...) caem de volta no array builtin. `.length`/`.size`
/// vão pelo handle-length genérico (ver rc_for_size em members.rs). fn_ptr null:
/// símbolos resolvem via a registração do namespace `collections`.
pub fn register_array_class_spec(e: &mut rts_engine::Engine) {
    use rts_engine::{AbiType, DefaultArg, FnPtr, Member, MemberFlags, MemberKind, Sig};
    fn m(name: &str, sig: Sig, symbol: &str, flags: MemberFlags) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::InstanceMethod,
            sig,
            symbol: symbol.to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: String::new(),
            doc: String::new(),
            pure: false,
            emit: None,
        }
    }
    let h = || Sig::new(vec![AbiType::Handle], AbiType::Handle);
    let amb = || Sig::new(vec![AbiType::Handle], AbiType::I64);
    // start? = 0, end? = i64::MIN (sentinela "até o fim") — defaults reais no
    // Registry em vez de hardcoded no codegen.
    let range3 = || {
        Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::I64],
            AbiType::Handle,
            vec![
                DefaultArg::Required,
                DefaultArg::Required,
                DefaultArg::Int(0),
                DefaultArg::Int(i64::MIN),
            ],
        )
    };
    // slice(start? = 0, end? = i64::MIN).
    let range2 = || {
        Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64, AbiType::I64],
            AbiType::Handle,
            vec![
                DefaultArg::Required,
                DefaultArg::Int(0),
                DefaultArg::Int(i64::MIN),
            ],
        )
    };
    let mut slice_member = m(
        "slice",
        range2(),
        "__RTS_FN_NS_COLLECTIONS_VEC_SLICE",
        MemberFlags::NONE,
    );
    slice_member.aliases = vec!["subarray".to_string()];
    // concat(...args) variádico: o emitter genérico empacota todos os args num
    // Vec<i64> de handles e chama VEC_CONCAT_VARIADIC(recv, vec).
    let mut concat_member = m(
        "concat",
        Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
        "__RTS_FN_NS_COLLECTIONS_VEC_CONCAT_VARIADIC",
        MemberFlags::NONE,
    );
    concat_member.variadic = true;
    let mut unshift_member = m(
        "unshift",
        Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64),
        "__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT_VARIADIC",
        MemberFlags::NONE,
    );
    unshift_member.variadic = true;
    // splice/toSpliced: empacota TODOS os args [start, count?, ...items] num Vec;
    // o runtime AUTO desempacota. splice mutaciona + devolve removidos;
    // toSpliced devolve cópia nova.
    let mut splice_member = m(
        "splice",
        Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
        "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_AUTO",
        MemberFlags::NONE,
    );
    splice_member.variadic = true;
    let mut tospliced_member = m(
        "toSpliced",
        Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
        "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_AUTO",
        MemberFlags::NONE,
    );
    tospliced_member.variadic = true;
    let mut length_member = m(
        "length",
        Sig::new(vec![AbiType::Handle], AbiType::I64),
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
        MemberFlags::NONE,
    );
    length_member.aliases = vec!["size".to_string()];
    // Comparator opcional (sort/toSorted) = default 0 → ordem default no runtime.
    // O arg fn é coergido pro func_addr pelo emitter genérico (ident de user fn
    // OU arrow inline hoisteada).
    let cb_opt = |sym: &str| {
        m(
            "x",
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::I64],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Int(0)],
            ),
            sym,
            MemberFlags::NONE,
        )
    };
    let mut sort_member = cb_opt("__RTS_FN_NS_COLLECTIONS_VEC_SORT");
    sort_member.name = "sort".to_string();
    let mut tosorted_member = cb_opt("__RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED");
    tosorted_member.name = "toSorted".to_string();
    e.class("Array")
        .doc("Array — métodos sem callback (pop/shift/reverse/toReversed/values/keys/entries/fill/copyWithin/with).")
        .member(m("at", Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64), "__RTS_FN_NS_COLLECTIONS_VEC_AT_AUTO", MemberFlags::AMBIGUOUS_RET))
        .member(m("pop", amb(), "__RTS_FN_NS_COLLECTIONS_VEC_POP", MemberFlags::AMBIGUOUS_RET))
        .member(m("shift", amb(), "__RTS_FN_NS_COLLECTIONS_VEC_SHIFT", MemberFlags::AMBIGUOUS_RET))
        .member(m("reverse", h(), "__RTS_FN_NS_COLLECTIONS_VEC_REVERSE", MemberFlags::NONE))
        .member(m("toReversed", h(), "__RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED", MemberFlags::NONE))
        .member(m("values", h(), "__RTS_FN_NS_COLLECTIONS_VEC_VALUES", MemberFlags::NONE))
        .member(m("keys", h(), "__RTS_FN_NS_COLLECTIONS_VEC_KEYS", MemberFlags::NONE))
        .member(m("entries", h(), "__RTS_FN_NS_COLLECTIONS_VEC_ENTRIES", MemberFlags::NONE))
        .member(m(
            "join",
            Sig::with_defaults(
                vec![AbiType::Handle, AbiType::Handle],
                AbiType::Handle,
                vec![DefaultArg::Required, DefaultArg::Int(0)],
            ),
            "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
            MemberFlags::NONE,
        ))
        .member(m("clear", Sig::new(vec![AbiType::Handle], AbiType::Void), "__RTS_FN_NS_COLLECTIONS_VEC_CLEAR", MemberFlags::NONE))
        .member(slice_member)
        .member(concat_member)
        .member(unshift_member)
        .member(length_member)
        .member(m("flat", Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64],
            AbiType::Handle,
            vec![DefaultArg::Required, DefaultArg::Int(1)],
        ), "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_DEPTH", MemberFlags::NONE))
        .member(splice_member)
        .member(tospliced_member)
        .member(sort_member)
        .member(tosorted_member)
        .member(m("flatMap", Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP", MemberFlags::NONE))
        .member(m("findLast", Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64), "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST", MemberFlags::AMBIGUOUS_RET))
        .member(m("findLastIndex", Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64), "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX", MemberFlags::NONE))
        // #305 iterator helpers eager: take(n)=slice(0,n) [runtime], drop(n)=
        // slice(n,fim) e toArray()=slice(0,fim) [VEC_SLICE via defaults].
        .member(m("take", Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_VEC_TAKE", MemberFlags::NONE))
        .member(m("drop", Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Handle,
            vec![DefaultArg::Required, DefaultArg::Required, DefaultArg::Int(i64::MIN)],
        ), "__RTS_FN_NS_COLLECTIONS_VEC_SLICE", MemberFlags::NONE))
        .member(m("toArray", range2(), "__RTS_FN_NS_COLLECTIONS_VEC_SLICE", MemberFlags::NONE))
        .member(m("fill", range3(), "__RTS_FN_NS_COLLECTIONS_VEC_FILL", MemberFlags::NONE))
        .member(m("copyWithin", range3(), "__RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN", MemberFlags::NONE))
        .member(m(
            "with",
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Handle),
            "__RTS_FN_NS_COLLECTIONS_VEC_WITH",
            MemberFlags::NONE,
        ))
        .done();
}
