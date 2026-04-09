# net — TCP Networking Nativo

## Status: implementado (v1)

## Visao geral

Namespace `net` expoe sockets TCP nativos via `std::net` do Rust.
Handles numericos (`u64`) sao gerenciados no runtime state (`NetHandle` enum).

## API

| Funcao          | Assinatura                                          | Descricao                                |
|-----------------|-----------------------------------------------------|------------------------------------------|
| `net.listen`    | `listen(host: str, port: u16): io.Result<u64>`      | Cria TCP listener no host:port           |
| `net.accept`    | `accept(listener: u64): io.Result<u64>`             | Aceita conexao (blocking)                |
| `net.connect`   | `connect(host: str, port: u16): io.Result<u64>`     | Conecta a servidor TCP                   |
| `net.read`      | `read(stream: u64, maxBytes?: usize): io.Result<str>` | Le ate maxBytes (default 4096)        |
| `net.write`     | `write(stream: u64, data: str): io.Result<usize>`   | Escreve dados, retorna bytes escritos    |
| `net.close`     | `close(handle: u64): void`                          | Fecha listener ou stream                 |
| `net.set_timeout` | `set_timeout(stream: u64, millis: u64): void`     | Timeout de read/write (0 = sem limite)   |
| `net.local_addr` | `local_addr(handle: u64): io.Result<str>`          | Endereco local "host:port"               |
| `net.peer_addr`  | `peer_addr(stream: u64): io.Result<str>`           | Endereco remoto "host:port"              |

## Arquivos

- `src/namespaces/net/mod.rs` — SPEC, dispatch, NetHandle, NetState, todas as operacoes TCP

## Design

- State proprio do namespace (`NET_STATE: OnceLock<Mutex<NetState>>`) — nao depende de `state.rs`
- Handles sao `u64` alocados incrementalmente no `NetState`
- `NetHandle` enum: `Listener(TcpListener)` ou `Stream(TcpStream)`
- `accept()` remove temporariamente o listener do state para nao segurar o lock durante I/O bloqueante
- Todos os erros retornam `io.Result<Err>` com mensagem descritiva
- `close()` remove o handle — o Drop do Rust fecha o socket

## Pendencias futuras

- [ ] UDP (`net.udp_bind`, `net.udp_send_to`, `net.udp_recv_from`)
- [ ] DNS lookup (`net.resolve`)
- [ ] Accept non-blocking / async via promises
- [ ] TLS (wrapping com rustls ou native-tls)
- [ ] HTTP client/server de alto nivel (como package TS sobre net primitivo)
