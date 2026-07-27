//! `#[rtse::variable]` struct-field expansion — the getter (and setter unless
//! `readonly`) for one scalar field. Split out from `member.rs` because a field
//! accessor is a much smaller, self-contained shape (no ctor/static/async/
//! value-receiver variants to consider) — folding it into the member-dispatch
//! machinery would drag field expansion through branches that never apply to it.

use quote::{format_ident, quote};
use syn::Type;

use crate::naming::to_camel;
use crate::types::scalar;

pub(crate) type FieldGen = (
    Vec<proc_macro2::TokenStream>,
    Vec<proc_macro2::TokenStream>,
    Vec<proc_macro2::Ident>,
);

/// Generate the getter (and setter unless `readonly`) extern + Member for one
/// `#[rtse::variable]` scalar field.
pub(crate) fn gen_field(
    _class: &str,
    class_upper: &str,
    self_ty: &Type,
    fname: &proc_macro2::Ident,
    fty: &Type,
    readonly: bool,
    doc: String,
) -> syn::Result<FieldGen> {
    let Some((abi, ext_ty, ts)) = scalar(fty) else {
        return Err(syn::Error::new_spanned(
            fty,
            "rtse variable: field must be f64/i64/i32/bool",
        ));
    };
    let is_bool = matches!(fty, Type::Path(p) if p.path.is_ident("bool"));
    let js_name = to_camel(&fname.to_string());
    // Field name VERBATIM (case-preserved) in the symbol — see `member` above.
    let field_name = fname.to_string(); // verbatim (case-preserved)
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();

    // Getter.
    let get_sym = format!("__RTS_FN_GL_{class_upper}_GET_{field_name}");
    let get_id = format_ident!("{}", get_sym);
    let read = quote!(::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| match __s {
        ::core::option::Option::Some(__s) => __s.#fname,
        ::core::option::Option::None => ::core::default::Default::default(),
    }));
    let get_ret = if is_bool { quote!((#read) as i64) } else { read };
    externs.push(quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #get_id(__recv: u64) -> #ext_ty { #get_ret }
    });
    let get_ts = format!("{js_name}: {ts}");
    members.push(quote! {
        .member(::rts_engine::Member {
            name: #js_name.into(),
            kind: ::rts_engine::MemberKind::InstanceGetter,
            sig: ::rts_engine::Sig::new(::std::vec![::rts_engine::AbiType::Handle], #abi),
            symbol: #get_sym.into(),
            fn_ptr: ::rts_engine::FnPtr(#get_id as *const u8),
            flags: ::rts_engine::MemberFlags::NONE,
            aliases: ::std::vec::Vec::new(),
            variadic: false,
            ts_signature: #get_ts.into(),
            doc: #doc.into(),
            pure: true,
            emit: ::core::option::Option::None,
        })
    });
    keep.push(get_id);

    // Setter (unless readonly).
    if !readonly {
        let set_sym = format!("__RTS_FN_GL_{class_upper}_SET_{field_name}");
        let set_id = format_ident!("{}", set_sym);
        let val = if is_bool { quote!((__v != 0)) } else { quote!(__v) };
        externs.push(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #set_id(__recv: u64, __v: #ext_ty) {
                ::rts_engine::heap::handles::with_rtse_mut::<#self_ty, _>(__recv, |__s| {
                    if let ::core::option::Option::Some(__s) = __s { __s.#fname = #val; }
                });
            }
        });
        members.push(quote! {
            .member(::rts_engine::Member {
                name: #js_name.into(),
                kind: ::rts_engine::MemberKind::InstanceSetter,
                sig: ::rts_engine::Sig::new(
                    ::std::vec![::rts_engine::AbiType::Handle, #abi],
                    ::rts_engine::AbiType::Void,
                ),
                symbol: #set_sym.into(),
                fn_ptr: ::rts_engine::FnPtr(#set_id as *const u8),
                flags: ::rts_engine::MemberFlags::NONE,
                aliases: ::std::vec::Vec::new(),
                variadic: false,
                ts_signature: ::std::string::String::new(),
                doc: ::std::string::String::new(),
                pure: false,
                emit: ::core::option::Option::None,
            })
        });
        keep.push(set_id);
    }

    Ok((externs, members, keep))
}
