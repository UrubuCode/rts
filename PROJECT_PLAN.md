## 🏗️ Arquitetura do Compilador RTS (Fluxo de Compilação)

O fluxo com AST própria permite controle total sobre como os tipos são preservados e utilizados pela máquina:

```
Código Fonte (.ts / .ts)
    → [Parser Próprio] → AST (com Tipos Preservados)
    → [RTS Front-end] → HIR (High-level Intermediate Representation)
    → [RTS Middle-end] → MIR (Middle-level IR) + Otimizações
    → [Cranelift] → Código de Máquina (Objeto .o)
    → [Linker Rust (crate object)] → Binário Final
```

### 1. Parsing: AST Própria (Sem SWC)

Diferente do TypeScript tradicional que **descarta tipos** durante a compilação para JavaScript, o RTS utiliza um **parser próprio** que preserva todas as informações de tipo na AST.

**Por que não usar SWC?**
- SWC remove tipos (type erasure) - `interface Usuario` vira nada no output
- SWC é otimizado para gerar JavaScript, não código nativo
- SWC não permite acesso runtime aos metadados de tipo

**O que nossa AST própria faz:**
- Preserva interfaces, type aliases, enums e genéricos como nós na AST
- Mantém anotações de tipo em variáveis, parâmetros e retornos
- Permite gerar metadados de tipo para reflexão em runtime
- Dá controle total sobre como cada construção TypeScript é compilada

### 2. Type System com Preservação (Diferencial do RTS)

O grande diferencial do RTS é que **tipos são cidadãos de primeira classe**:

| Recurso | TypeScript Normal | RTS (nosso compilador) |
|---------|-------------------|------------------------|
| `interface Usuario` | Some na compilação | ✅ Vira metadado no binário |
| `typeof` em runtime | Apenas para valores JS | ✅ Funciona para tipos customizados |
| `instanceof` com interface | ❌ Não funciona | ✅ Sim, via type registry |
| Reflexão de campos | ❌ | ✅ `getFields(usuario)` |
| Validação runtime | Precisa de Zod/Yup | ✅ Direto da definição |
| Serialização automática | Manual | ✅ Baseada nos metadados |

### 3. HIR e MIR (Análise e Otimizações)

A AST própria é transformada em **HIR (High-level IR)** mais limpa, ainda com informações de tipo.

- **Resolução de escopos:** Identifica variáveis, funções e tipos no escopo correto
- **Verificação de tipos:** Garante compatibilidade entre tipos preservados
- **Desaçucaramento:** Transforma `for...of` em loops simples, `async/await` em state machines
- **Geração de Type IDs:** Atribui identificadores únicos a cada tipo para reflexão

**MIR (Mid-level IR):**
- Versão de baixo nível, similar ao que o Rust usa internamente
- **Análise de escape:** Decide stack vs heap
- **Monomorfização de genéricos:** Gera versões específicas para cada tipo usado
- **Inlining:** Expande funções pequenas no local de chamada

### 4. Geração de Código: Cranelift

Com o MIR otimizado, traduzimos para **CLIF (Cranelift IR)** e geramos código de máquina:

- **Vantagem:** Cranelift é muito mais rápido que LLVM para compilações de desenvolvimento
- **Metadados de tipo:** Emitidos como dados estáticos no binário (seção `.rodata`)
- **Type Registry:** Código gerado inclui registro de tipos para reflexão em runtime

### 5. Linkagem: Backend 100% Rust

Para gerar o executável final sem dependências externas:

- Usamos um **linker/pacotador em Rust** baseado na crate `object`
- Multiplataforma e self-contained
- Linkagem estática por padrão, com suporte a monomorfização

---

## 📁 Estrutura de Diretórios do Projeto (`PROJECT_MAP.md`)

```text
rts/                            # Raiz do projeto Rust
├── Cargo.toml                  # Workspace configuration
├── README.md                   # Documentação
├── PROJECT_MAP.md              # Este arquivo
│
├── src/                        # Compilador RTS
│   ├── main.rs                 # CLI: rts build, rts run, rts repl
│   ├── lib.rs                  # Biblioteca pública
│   │
│   ├── parser/                 # ⭐ PARSER PRÓPRIO (sem SWC)
│   │   ├── mod.rs
│   │   ├── lexer.rs            # Tokenização (logos/ logos)
│   │   ├── grammar.pest        # Gramática Pest
│   │   ├── ast.rs              # AST COM TIPOS PRESERVADOS
│   │   └── span.rs             # Informações de posição (para erros)
│   │
│   ├── type_system/            # ⭐ CORAÇÃO: Sistema de tipos
│   │   ├── mod.rs
│   │   ├── types.rs            # Representação dos tipos (preservados)
│   │   │   # Ex: TypeKind::Interface { name, fields, methods }
│   │   ├── metadata.rs         # Gerador de metadados para o binário
│   │   ├── checker.rs          # Verificador de tipos (compile-time)
│   │   ├── resolver.rs         # Resolução de nomes de tipos
│   │   └── runtime_info.rs     # Como tipos são acessados em runtime
│   │
│   ├── hir/                    # HIR (tipos ainda presentes)
│   │   ├── mod.rs
│   │   ├── lower.rs            # AST -> HIR
│   │   ├── nodes.rs            # Definição dos nós HIR
│   │   └── annotations.rs      # Anotações de tipo na HIR
│   │
│   ├── mir/                    # MIR (otimizações)
│   │   ├── mod.rs
│   │   ├── build.rs            # HIR -> MIR
│   │   ├── optimize.rs         # Análise de escape, inlining
│   │   ├── monomorphize.rs     # Expansão de genéricos
│   │   └── cfg.rs              # Grafo de fluxo de controle
│   │
│   ├── codegen/                # Geração de código
│   │   ├── mod.rs
│   │   ├── cranelift/          # Backend Cranelift
│   │   │   ├── clif_builder.rs # MIR -> CLIF
│   │   │   ├── type_layout.rs  # Como tipos viram layout de memória
│   │   │   └── metadata.rs     # Emite metadados de tipo no .rodata
│   │   └── object.rs           # Geração do arquivo .o
│   │
│   ├── linker/                 # Linkagem nativa em Rust
│   │   ├── mod.rs
│   │   └── object_linker.rs    # Gera binário por formato (COFF/ELF/Mach-O)
│   │
│   ├── diagnostics/            # Erros bonitos
│   │   ├── mod.rs
│   │   ├── reporter.rs         # Formatação com cores
│   │   └── suggestions.rs      # Sugestões de correção
│   │
│   └── cli/                    # Comandos CLI
│       ├── mod.rs
│       ├── build.rs            # rts build
│       ├── run.rs              # rts run
│       └── repl.rs             # rts repl (modo interativo)
│
├── runtime/                    # ⭐ Standard Library com reflexão
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── type_registry.rs    # Registro global de tipos (runtime)
│   │   ├── reflection.rs       # API: getTypeMetadata(), instanceof()
│   │   ├── alloc.rs            # Alocador de memória
│   │   ├── panic.rs            # Panic handler
│   │   └── builtins.rs         # console.log, etc.
│   │
│   └── rts_macros/             # Macros para metadados de tipo
│       ├── Cargo.toml
│       └── src/lib.rs          # #[derive(TypeMetadata)]
│
├── tests/                      # Testes
│   ├── compile_tests/          # Deve compilar
│   │   ├── basic/
│   │   └── types/              # Testes de sistema de tipos
│   └── run_tests/              # Compila e executa
│       ├── reflection/         # Testes de reflexão em runtime
│       └── generics/
│
├── examples/                   # Exemplos .ts
│   ├── hello_world.ts
│   ├── reflection.ts          # Usando getTypeMetadata()
│   └── generic_stack.ts       # Stack<T> com monomorfização
│
└── target/                     # Build artifacts (ignorado)
```

---

## 📝 Roteiro de Implementação (Roadmap)

### Semana 1-2: Fundação e Parser
- [ ] Setup do projeto e workspace
- [ ] Implementar lexer (logos ou pest)
- [ ] Definir gramática completa (grammar.pest)
- [ ] Implementar AST com preservação de tipos
- [ ] Testes de parsing para sintaxe básica

### Semana 3-4: Type System
- [ ] Implementar representação de tipos (`type_system/types.rs`)
- [ ] Type checker básico (compatibilidade entre tipos)
- [ ] Resolução de escopos e nomes
- [ ] Gerador de Type IDs (hash dos tipos)
- [ ] Testes com interfaces e tipos genéricos

### Semana 5-6: HIR e MIR
- [ ] AST → HIR (lowering)
- [ ] Implementar HIR nodes
- [ ] HIR → MIR (build)
- [ ] Análise de escape básica
- [ ] Simplificação de CFG

### Semana 7-8: Codegen com Cranelift
- [ ] Integração do Cranelift
- [ ] MIR → CLIF (tradução)
- [ ] Emissão de código de máquina (.o)
- [ ] Geração de metadados de tipo no .rodata
- [ ] Compilar função `add(a, b) → c`

### Semana 9-10: Linkagem e Runtime
- [ ] Integrar linker/pacotador em Rust por formato
- [ ] Linkagem estática do binário
- [ ] Runtime básico (type registry, reflection)
- [ ] `println!` via FFI
- [ ] Exemplo hello_world.ts funcionando

### Semana 11-12: Features Avançadas
- [ ] Monomorfização de genéricos
- [ ] Reflection em runtime (`getTypeMetadata()`)
- [ ] Pattern matching em tipos
- [ ] Serialização automática baseada em metadados
- [ ] Testes de performance

---

## 🎯 Exemplo de Código RTS (O Que Nosso Compilador Vai Rodar)

```typescript
// reflection.ts - Exemplo que NÃO seria possível em TypeScript normal!
interface Usuario {
  id: number;
  nome: string;
  email: string;
}

function cloneComReflexao<T>(obj: T): T {
  // ⭐ Em RTS, isso funciona porque tipos são preservados!
  const fields = getTypeMetadata<T>().fields;
  const novo: any = {};
  for (const field of fields) {
    novo[field.name] = obj[field.name];
  }
  return novo as T;
}

function isUsuario(obj: any): obj is Usuario {
  const metadata = getTypeMetadata<Usuario>();
  return metadata.fields.every(f => f.type.matches(obj[f.name]));
}

const user: Usuario = { id: 1, nome: "João", email: "joao@email.com" };
const cloned = cloneComReflexao(user);

console.log("Clone:", cloned);
console.log("É usuário?", isUsuario(cloned));
```

Este código seria compilado para um binário nativo, com metadados de tipo embutidos e reflexão funcionando em runtime - algo que o TypeScript normal jamais poderia fazer.

---

## 🔑 Principais Decisões de Design

| Decisão | Motivo |
|---------|--------|
| **AST própria** | Preservar tipos para uso em runtime (reflexão) |
| **Sem SWC** | SWC faz type erasure, incompatível com nossos objetivos |
| **Cranelift** | Mais rápido que LLVM para compilações debug |
| **Linker em Rust (crate object)** | Sem depender de comandos externos no sistema |
| **Runtime separado** | Biblioteca padrão linkada estaticamente (como Go/Rust) |
| **Monomorfização** | Performance máxima para genéricos (como Rust) |

---

Este plano está alinhado com sua visão de criar um "TypeScript nativo" onde tipos são preservados e utilizados pela máquina, não apenas para verificação em tempo de desenvolvimento.


