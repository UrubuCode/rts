# PASSO 6 — MIR: `MIRLocation` Preservation

## Objetivo

Adicionar `MIRLocation` ao MIR para preservar a localização de source que vem do HIR.
O MIR é a camada que conecta HIR (semântico) ao Cranelift (bytesCode). Preservando
localização aqui, o Cranelift pode emitir DWARF e o OmetaWriter pode registrar
offsets precisos.

## Por que o MIR?

O MIR já trabalha com instruções sequenciais e basic blocks. É onde a compactação
faz sentido: múltiplas instruções na mesma linha → uma entrada no .ometa em vez de N.

## Estrutura a adicionar

### `src/mir/mod.rs`

```rust
/// Localização no arquivo fonte preservada através do pipeline.
#[derive(Debug, Clone, Default)]
pub struct MIRLocation {
    pub file_id: u32,       // ID no sourcemap (índice em OmetaWriter.sources)
    pub line: u32,
    pub column: u32,
    pub byte_offset: u64,   // preenchido pelo Cranelift após emissão
}
```

Adicionar a `TypedMirFunction`:
```rust
pub source_file: Option<String>,  // arquivo TypeScript de origem
pub source_line: u32,             // linha da declaração da função
```

Adicionar a `MirInstruction` um wrapper `WithLocation`:
```rust
pub struct LocatedInstruction {
    pub instruction: MirInstruction,
    pub loc: Option<MIRLocation>,
}
```

Ou alternativamente: mudar `TypedBasicBlock.instructions` para
`Vec<(MirInstruction, Option<MIRLocation>)>`.

## Decisão de implementação

Para minimizar breaking changes, adicionar campos `source_file` e `source_line`
somente a `TypedMirFunction`. A granularidade de instrução fica para iterações
futuras — por basic block é suficiente para o .ometa da versão inicial.
