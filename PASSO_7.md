# PASSO 7 — Codegen: Typed Calls + Emissão de `.ometa`

## Objetivo

1. Integrar `OmetaWriter` ao pipeline de codegen typed.
2. Quando `CompilationProfile::is_development()`, emitir `.ometa` ao lado do `.o`.
3. Adicionar `CompileOptions` ao `generate_typed_object` para controlar emissão de debug info.

## Por que o codegen emite .ometa?

Cranelift conhece os offsets finais das funções no objeto. É aqui que o
`OmetaWriter` pode ser alimentado com `add_function(name, offset, size, source, line)`.
Sem essa etapa, o .ometa ficaria incompleto.

## Mudanças em `src/codegen/mod.rs`

```rust
pub fn generate_typed_object(
    mir: &TypedMirModule,
    output: &Path,
    emit_entrypoint: bool,
    options: &CompileOptions,        // ← novo parâmetro
) -> Result<ObjectArtifact> {
    let bytes = lower_typed_to_native_object(...);
    let artifact = object::write_object_file(output, &bytes)?;

    // Emite .ometa somente em modo desenvolvimento
    if options.profile.is_development() {
        let mut ometa = OmetaWriter::new("development", source_root);
        for func in &mir.functions {
            if let Some(ref src) = func.source_file {
                ometa.add_function(&func.name, 0, 0, src, func.source_line);
            }
        }
        if !ometa.is_empty() {
            ometa.write_to(output)?;
        }
    }
    Ok(artifact)
}
```

## Mudanças em chamadores

`generate_typed_object` recebe `&CompileOptions` — atualizar chamadas em `pipeline.rs`.

## Nota

Offsets de função (`byte_offset`) ficam como 0 por enquanto — a integração com
Cranelift para offsets exatos é a etapa seguinte (fora do escopo desta feature inicial).
