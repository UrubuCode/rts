# PASSO 5 — HIR: `SourceLocation` Tracking

## Objetivo

Adicionar `SourceLocation` ao HIR para que cada node carregue sua posição no arquivo
TypeScript original. Isso é o primeiro passo do pipeline de debug info: sem localização
no HIR, as camadas posteriores (MIR, Cranelift) não têm de onde extrair informação.

## Por que no HIR?

O HIR tem visibilidade sintática completa e acesso ao AST do SWC (que já contém
informações de span/linha/coluna). É a camada certa para anexar localização — o
MIR trabalha com instruções sequenciais e já seria tarde para inferir posição.

## Estrutura a adicionar

### `src/hir/nodes.rs`

```rust
#[derive(Debug, Clone, Default)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}
```

Adicionar campo `loc: Option<SourceLocation>` em:
- `HirFunction` — localização da declaração da função
- `HirClass` — localização da declaração da classe

## `src/hir/mod.rs`

Re-exportar `SourceLocation` como `pub use nodes::SourceLocation`.

## Nota

Os campos são `Option<SourceLocation>` — nodes criados sem informação de span
(ex: gerados sinteticamente pelo HIR) mantêm `None`. O MIR trata `None` como
"sem debug info para esta instrução".
