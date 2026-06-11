//! Assinaturas ABI de um membro registrado.

use crate::abi::AbiType;

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
}

impl Sig {
    /// Constrói uma assinatura. Prefira o macro [`sig!`](crate::sig).
    pub fn new(args: Vec<AbiType>, returns: AbiType) -> Self {
        Self { args, returns }
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
