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
packages/
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
