---
name: get-release
description: Baixa um binário `rts` já compilado dos GitHub Releases (Windows/Linux/macOS) para rodar programas sem gastar minutos em `cargo build --release`. Use quando for preciso executar `rts run`/`test`/`compile` e não houver `target/release/rts.exe`, ou quando se quiser reproduzir o comportamento de um commit publicado em vez do working tree.
---

# Baixar um release do `rts`

Um build release aqui leva minutos (`lto = "thin"`, `codegen-units = 1`). Quando
o objetivo é **executar** um programa e não testar uma alteração local, o binário
do release responde a mesma pergunta em segundos.

Ele responde por um **commit publicado**, não pelo working tree: se a pergunta é
"a minha mudança funciona?", este não é o binário — use `cargo run -- run`.

## 1. Escolher o release

```bash
gh release list -L 5
```

A tag é `v0.0-<AAAAMMDDHHMM>`, ordenada por data; `Latest` é a primeira. Para
saber de qual commit ela veio (necessário sempre que o resultado for citado):

```bash
gh release view <tag> --json tagName,publishedAt,body
```

## 2. Baixar o asset da plataforma

Os assets são nomeados `rts-<tag>-<Plataforma>`:

| plataforma | padrão `-p` |
|---|---|
| Windows | `*Windows-X64.exe` |
| Linux | `*Linux-X64` |
| macOS ARM | `*macOS-ARM64` |

```bash
gh release download <tag> -p '*Windows-X64.exe' -O <destino>/rts-<tag>.exe --clobber
```

Duas coisas que custam uma tentativa cada:

- **`gh` precisa ser chamado de dentro do repositório.** Fora dele o comando
  morre com `fatal: not a git repository`, mesmo passando `-O` para fora. Rode
  a partir de `E:\rts` e mande o `-O` para onde quiser.
- **Não existe `--version`.** Para conferir que o binário abre, use `rts --help`
  ou rode um programa de uma linha.

Guarde o `.exe` no scratchpad da sessão, nunca no projeto do usuário: é um
artefato de 50 MB e não pertence a nenhum repositório.

## 3. Usar

O binário é a mesma CLI:

```bash
<bin> run file.ts        # executa
<bin> -e "console.log(1)"
<bin> ir file.ts         # o IR deste motor
<bin> test tests/x.test.ts
```

## Ao relatar um resultado

Diga **de qual tag e commit** o binário veio. Um release é uma foto de `main` no
momento do build, então "falhou no motor novo" sem a tag é uma afirmação sem
data — e a diferença entre a tag e o `HEAD` local é exatamente o que explica um
resultado que não bate com o working tree.
