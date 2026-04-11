Design: .rtslib como namespace externo

  O que seria um .rtslib

  Um arquivo-container (tar.gz ou zip renomeado) com:

  ssh2.rtslib
  ├── manifest.json                     — metadados: namespace, versão, callees, ABI version
  ├── x86_64-pc-windows-msvc/
  │   └── ssh2_namespace.o              — objeto compilado pro target
  ├── x86_64-unknown-linux-gnu/
  │   └── ssh2_namespace.o
  ├── aarch64-apple-darwin/
  │   └── ssh2_namespace.o
  └── types/
      └── ssh2.d.ts                      — tipos TS gerados pelo crate

  manifest.json:
  {
    "schema": 1,
    "namespace": "ssh2",
    "version": "0.1.0",
    "rts_abi_version": 1,
    "callees": ["ssh2.connect", "ssh2.exec", "ssh2.close"],
    "targets": ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]
  }

  ---
  O crate rts_namespace (proc-macro)

  O autor do .rtslib escreve Rust puro com a macro — sem precisar conhecer o internals do RTS:

  // Cargo.toml do crate do usuário
  // [dependencies]
  // rts_namespace = "0.1"

  use rts_namespace::namespace;

  #[namespace(name = "ssh2")]
  mod ssh {
      use rts_namespace::prelude::*;

      /// Opens an SSH connection. Returns a session handle.
      #[callee("ssh2.connect")]
      pub fn connect(
          host: StrArg,   // (ptr: i64, len: i64) — convenção RTS
          port: i64,
      ) -> u64 {           // handle opaco
          // implementação real com a crate ssh2 do crates.io
          todo!()
      }

      /// Executes a command on a session handle.
      #[callee("ssh2.exec")]
      pub fn exec(session: u64, cmd: StrArg) -> StrHandle {
          todo!()
      }
  }

  A macro gera automaticamente:
  - #[unsafe(no_mangle)] pub extern "C" fn __rts_ssh2_connect(ptr: i64, len: i64, port: i64) -> u64
  - pub const SPEC: NamespaceSpec
  - pub const MEMBERS: &[NamespaceMember]
  - O .d.ts como string embutida (via include_str! de arquivo gerado em build.rs)

  O .o é gerado com cargo build --release targeting cada triple. O empacotamento em .rtslib seria via uma CLI auxiliar rts_namespace pack.

  ---
  Integração no pipeline

  package.json do projeto do usuário:
  {
    "name": "meu-app",
    "rtslibs": [
      "https://raw.githubusercontent.com/.../ssh2.rtslib",
      "./libs/meu_custom.rtslib"
    ]
  }

  Durante rts run / rts compile:

  1. Lê rtslibs do package.json
  2. Baixa e verifica SHA256 (hash declarado no manifest)
  3. Extrai e cacheia em node_modules/.rts/libs/<namespace>/
  4. Seleciona .o para o target triple atual
  5. Injeta o .d.ts em node_modules/.rts/builtin/<namespace>/
  6. Registra os callees do manifest no NamespaceUsage — o linker inclui o .o junto com os outros namespace objects

  O import "rts:ssh2" ou "ssh2" resolve para o .d.ts gerado, exatamente como "rts:fs".

  ---
  Onde isso difere do .node do Node

  ┌───────────────┬─────────────────────────────┬────────────────────────────────────────┐
  │               │       .node (Node.js)       │             .rtslib (RTS)              │
  ├───────────────┼─────────────────────────────┼────────────────────────────────────────┤
  │ Carregamento  │ dlopen em runtime           │ linkagem estática em compile time      │
  ├───────────────┼─────────────────────────────┼────────────────────────────────────────┤
  │ ABI           │ N-API (V8 types)            │ tipos de máquina puros (i64, f64, u64) │
  ├───────────────┼─────────────────────────────┼────────────────────────────────────────┤
  │ Overhead      │ indirect call via N-API     │ zero — símbolo direto no binário       │
  ├───────────────┼─────────────────────────────┼────────────────────────────────────────┤
  │ Portabilidade │ .node por plataforma        │ .o por triple no container             │
  ├───────────────┼─────────────────────────────┼────────────────────────────────────────┤
  │ Segurança     │ dlopen abre a qualquer hora │ link time — hash verificado antes      │
  └───────────────┴─────────────────────────────┴────────────────────────────────────────┘

  O .rtslib é mais parecido com um crate estático pré-compilado com metadados de namespace do que com um addon dinâmico. O binário final não tem nenhuma dependência de carregamento dinâmico.

  ---
  Considerações de segurança

  - O manifest.json declara um rts_abi_version — o RTS recusa .rtslib com ABI version incompatível
  - Hash SHA256 do arquivo declarado no package.json ou num lockfile .rtslibs.lock (análogo ao bun.lock)
  - .rtslib de URL sem hash declarado → warning ou erro dependendo do profile

  ---
  Viabilidade atual

  O que já existe e serve de base:
  - emit_selected_namespace_objects() já itera sobre objetos e os passa pro linker — adicionar objetos de .rtslib é trivial aqui
  - NamespaceUsage já rastreia callees por fonte — o manifest do .rtslib alimenta isso diretamente
  - ObjectCacheMeta / is_cached_object_valid — o cache de .o já existe, só precisa de uma variante para .rtslib

  O que precisaria ser criado do zero:
  - O crate rts_namespace (proc-macro + runtime types)
  - O formato de empacotamento .rtslib (simples zip com manifest)
  - rts_namespace pack (CLI para empacotar multi-target)
  - Resolução de rtslibs no package.json
  - Verificação de hash e lockfile

  A ideia é arquiteturalmente sólida e não quebra nenhuma invariante existente — o .rtslib é só mais um .o pro linker, com metadados extras para o RTS saber quais callees ele expõe.