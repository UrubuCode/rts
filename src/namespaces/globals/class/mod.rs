/// Módulo responsável pela lógica de `this` em funções e classes.
/// 
/// Este módulo centraliza:
/// - Detecção se uma função possui parâmetro `this`
/// - Resolução de ownership de métodos em hierarquias de classe
/// - Suporte a chamadas via `.call()`, `.apply()`, `.bind()`
/// - Reificação de funções com contexto `this` correto

use crate::codegen::lower::ctx::FnCtx;

/// Retorna true quando a função compilada tem `this` como primeiro parâmetro.
/// Isso ocorre em:
/// - Métodos de classe não-estáticos: nome começa com `__class_` e não contém `_static_`
/// - Funções construtoras (não são synthetic `__init`)
/// - Funções chamadas via `.call()` ou `.apply()` (sempre têm thisArg)
pub fn fn_has_this_param(fn_name: &str) -> bool {
    // Métodos de instância de classe
    if fn_name.starts_with("__class_") && !fn_name.contains("_static_") {
        return !fn_name.ends_with("__init");
    }
    false
}

/// Resolve qual classe na hierarquia possui o método especificado.
/// Retorna o nome da classe dona do método, ou None se não encontrado.
pub fn resolve_method_owner(ctx: &FnCtx, class_name: &str, method_name: &str) -> Option<String> {
    // Verifica na própria classe
    if let Some(meta) = ctx.classes.get(class_name) {
        if meta.methods.iter().any(|m| m == method_name) {
            return Some(class_name.to_string());
        }
        
        // Verifica na cadeia de protótipos (herança)
        if let Some(parent_class) = &meta.parent_class {
            return resolve_method_owner(ctx, parent_class, method_name);
        }
    }
    None
}

/// Determina se uma função deve ser tratada como tendo parâmetro `this` explícito.
/// 
/// # Argumentos
/// * `fn_name` - Nome da função no formato interno (ex: `__class_Animal_init`)
/// * `call_method` - Método usado na chamada (ex: "call", "apply", "bind", ou None para chamada direta)
/// 
/// # Retorno
/// `true` se a função deve receber `this` como primeiro parâmetro
pub fn should_have_this_param(fn_name: &str, call_method: Option<&str>) -> bool {
    // Chamadas via .call()/.apply() sempre passam thisArg explicitamente
    if matches!(call_method, Some("call") | Some("apply")) {
        return true;
    }
    
    // Verifica se é um método de instância
    fn_has_this_param(fn_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_has_this_param() {
        // Métodos de instância
        assert!(fn_has_this_param("__class_Animal_speak"));
        assert!(fn_has_this_param("__class_Dog_bark"));
        
        // Métodos estáticos
        assert!(!fn_has_this_param("__class_Animal_static_create"));
        assert!(!fn_has_this_param("__class_Dog_static_from_name"));
        
        // Construtores
        assert!(!fn_has_this_param("__class_Animal__init"));
        assert!(!fn_has_this_param("__class_Dog__init"));
        
        // Funções normais
        assert!(!fn_has_this_param("__RTS_FN_GL_STRING_TO_UPPER_CASE"));
        assert!(!fn_has_this_param("__RTS_MAIN"));
    }

    #[test]
    fn test_should_have_this_param() {
        // Chamada direta em método de instância
        assert!(should_have_this_param("__class_Animal_speak", None));
        
        // Chamada via .call() em qualquer função
        assert!(should_have_this_param("my_function", Some("call")));
        assert!(should_have_this_param("__RTS_MAIN", Some("call")));
        
        // Chamada via .apply() em qualquer função
        assert!(should_have_this_param("my_function", Some("apply")));
        
        // Chamada via .bind() não força thisParam, depende da função
        assert!(!should_have_this_param("my_function", Some("bind")));
        assert!(should_have_this_param("__class_Animal_speak", Some("bind")));
        
        // Método estático nunca tem this
        assert!(!should_have_this_param("__class_Animal_static_create", None));
        assert!(!should_have_this_param("__class_Animal_static_create", Some("call")));
    }
}
