//! Assinaturas ABI de um membro registrado.

use crate::abi::AbiType;
use crate::abi::member::DefaultArg;

/// Assinatura de um membro: tipos dos argumentos (na ordem do `extern "C"`,
/// incluindo o `Handle` do receiver no slot 0 para métodos de instância) + o
/// tipo de retorno. O codegen deriva a assinatura Cranelift daqui, exatamente
/// como `crate::abi::signature::lower_member` faz para as rows antigas.
#[derive(Debug, Clone, PartialEq)]
pub struct Sig {
    /// Tipos dos argumentos, em ordem. `StrPtr` expande para dois slots
    /// `(ptr, len)` no lowering.
    pub args: Vec<AbiType>,
    /// Tipo de retorno. `Void` = sem retorno.
    pub returns: AbiType,
    /// Política de default por argumento, paralela a `args` (mesmo comprimento,
    /// ou vazia = "todos requeridos"). Deixa o Registry expressar o valor que o
    /// emissor genérico injeta quando o caller omite um arg trailing (ex.:
    /// `toExponential(d? = -1)`, `slice(end? = SENTINEL)`) — em vez de o codegen
    /// hardcodar. Vazio na imensa maioria. `Sig::new` deixa vazio; use
    /// [`Sig::with_defaults`] quando houver default não-trivial.
    pub default_args: Vec<DefaultArg>,
}

impl Sig {
    /// Constrói uma assinatura sem defaults (todos os args requeridos / o pad
    /// genérico usa zero-do-tipo = `undefined`). Prefira o macro [`sig!`].
    pub fn new(args: Vec<AbiType>, returns: AbiType) -> Self {
        Self { args, returns, default_args: Vec::new() }
    }

    /// Constrói uma assinatura com política de default por argumento (paralela a
    /// `args`). Para os membros cujo default omitido NÃO é zero-do-tipo.
    pub fn with_defaults(
        args: Vec<AbiType>,
        returns: AbiType,
        default_args: Vec<DefaultArg>,
    ) -> Self {
        Self { args, returns, default_args }
    }

    /// Número de parâmetros explícitos (TS-visíveis) de um método de instância:
    /// `args` menos o `Handle` implícito do receiver no slot 0.
    pub fn explicit_arity(&self) -> usize {
        self.args.len().saturating_sub(1)
    }
}

/// Açúcar ergonômico para [`Sig`] — devolve a concisão da macro sem proc-macro.
///
/// ```
/// use rts_engine::sig;
/// let s = sig!(StrPtr, I64 => Handle); // (string, number) -> handle
/// let v = sig!(I64 => Void);           // (number) -> void
/// let c = sig!(=> F64);                // () -> number  (ex: constante)
/// assert_eq!(s.args.len(), 2);
/// ```
#[macro_export]
macro_rules! sig {
    ( $( $arg:ident ),* => $ret:ident ) => {
        $crate::Sig::new(
            ::std::vec![ $( $crate::AbiType::$arg ),* ],
            $crate::AbiType::$ret,
        )
    };
}
