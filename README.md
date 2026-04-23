# RTS

Bootstrap inicial do compilador RTS com modulo builtin `"rts"` fornecido pelo runtime em Rust.

## Modos de compilacao

- `--development` / `-d`: modo padrao, com trace route detalhado de erro (imports/modulos).
- `--production` / `-p`: erros resumidos por codigo (`RTSXXXXXXXX`) e payload sem dados de trace TS.
- `--debug` / `-D`: adiciona detalhes extras em cima do modo selecionado.

## Uso CLI

Flags:

```txt
--development -d
--production  -p
--debug       -D
```

Comandos:

```bash
rts main.ts
rts --production main.ts
rts build main.ts
rts build -p main.ts
rts init
rts init my-app
rts apis
```

Tambem funciona via Cargo:

```bash
cargo run -- examples/console.ts
cargo run -- build -p examples/console.ts target/console
cargo run -- init my-app
cargo run -- apis
```

## `rts init`

Gera projeto base perguntando o nome (ou recebe por argumento):

- `src/main.ts`
- `package.json`
- `README.md` com cabecalho `### Project <name> generated with rts <version>`

## Pacotes TS

Estrutura suportada:

```txt
builtin/
  console/
    main.ts
    package.json
  std/
    main.ts
    package.json
  fs/
    main.ts
    package.json
  process/
    main.ts
    package.json
```

Formato de `package.json`:

```json
{
  "name": "console",
  "version": "1.0.0",
  "main": "main.ts",
  "dependencies": {
    "rtst": "npm:1.0.0",
    "lib2": "https://example.com/lib2.ts"
  }
}
```

Suportes atuais:

- import relativo (`./`, `../`)
- import de pacote do workspace (`import { x } from "console"`)
- import builtin (`"rts"`)
- import de URL externa (`https://...`)
- dependencia em `package.json` com `npm:<versao>`, URL externa, ou path local

## Cache de modulos

Variavel opcional:

```txt
RTS_MODULES_PATH=~
```

Layout:

```txt
~/.rts/modules/<distribution>/<modulename>/<version>/<files>
```

Exemplo:

```txt
~/.rts/modules/npm/fs/0.0.1/*
```

Sem `RTS_MODULES_PATH`, o padrao tambem e `~/.rts/modules`.

## Linker nativo (Fase 1)

O linker agora suporta estrategia automatica:

- `RTS_LINKER_BACKEND=auto` (padrao): tenta linker do sistema e cai para backend manual (`object`) se falhar.
- `RTS_LINKER_BACKEND=system`: exige linker do sistema.
- `RTS_LINKER_BACKEND=object`: usa apenas backend manual atual.

No modo `auto/system`, o RTS tenta nesta ordem:

- `~/.rts/toolchains/<target>/bin`
- `rustup` / sysroot do `rustc` (`rust-lld`)
- `PATH` do sistema
- download automatico do Rust Dist estavel para extrair `rust-lld` por target

Configuracoes adicionais:

- `RTS_TARGET=<target-triple>` para escolher target explicitamente.
- `RTS_TOOLCHAINS_PATH=<path>` para alterar cache de toolchains (padrao `~/.rts/toolchains`).
- `RTS_LINKER_DOWNLOAD_URL=<template>` para provisionar linker automaticamente no cache (o template aceita `{target}` e `{binary}`).
- `RTS_LINKER_SHA256=<hash>` para validar o binario baixado.

## AOT nativo

O codegen agora tenta emitir objeto nativo real (`.o/.obj`) via Cranelift Object por padrao.
Se a emissao nativa falhar, o RTS cai para o payload textual CLIF para nao quebrar o fluxo.

## Dependencias do compilador

- `anyhow`
- `object`
- `serde`
- `serde_json`
- `ureq`

## Pendencias principais do runtime

- camada ABI/FFI estavel (chamadas C e carga de simbolos)
- ponte de syscall real para filesystem/processo
- scheduler assincrono e timers
- contrato de seguranca de memoria (`alloc/dealloc`)
- formato binario de pacotes precompilados
- networking (TCP/UDP/DNS/HTTP)
- diagnosticos estruturados + sourcemap em AOT
