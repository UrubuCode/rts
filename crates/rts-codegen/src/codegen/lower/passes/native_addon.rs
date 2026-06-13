//! (N-API) Bind de imports `.node` no codegen.
//!
//! `Program::native_addon_imports` mapeia nome local → path absoluto do `.node`.
//! Como reescrever a assinatura de `FnCtx::new` (e todos os call sites) só para
//! propagar esse mapa seria invasivo, guardamos os bindings num thread-local
//! populado no início de `compile_program`. O `lower_ident_expr` consulta esse
//! mapa: se o ident é um addon nativo, emite uma chamada a
//! `__RTS_FN_NS_NAPI_LOAD_ADDON(path)` cujo retorno (handle do `exports`) é o
//! valor do ident. O loader runtime é idempotente por path (carrega uma vez),
//! então emitir a chamada a cada referência é correto e barato.
//!
//! Ver docs/specs/napi-implementation.md (Etapa 4).

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static NATIVE_ADDON_IMPORTS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Substitui o mapa de bindings de addon para a compilação atual. Chamado no
/// início de `compile_program` a partir de `Program::native_addon_imports`.
pub fn set_native_addon_imports(map: &HashMap<String, String>) {
    NATIVE_ADDON_IMPORTS.with(|cell| {
        *cell.borrow_mut() = map.clone();
    });
}

/// Limpa o mapa (evita vazamento entre compilações na mesma thread).
pub fn clear_native_addon_imports() {
    NATIVE_ADDON_IMPORTS.with(|cell| cell.borrow_mut().clear());
}

/// Path absoluto do `.node` para um nome local de import, se for um addon.
pub fn native_addon_path(local_name: &str) -> Option<String> {
    NATIVE_ADDON_IMPORTS.with(|cell| cell.borrow().get(local_name).cloned())
}

/// `true` se o programa importa ao menos um addon `.node`. Usado para só tentar
/// o dispatch de método de instância nativa quando faz sentido (evita overhead
/// e regressão em programas sem addons).
pub fn any_native_addon() -> bool {
    NATIVE_ADDON_IMPORTS.with(|cell| !cell.borrow().is_empty())
}
