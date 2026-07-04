// TCP echo end-to-end via subprocess. Toda a logica vive dentro do
// test() pra alinhar com o padrao dos outros testes que usam
// process.spawn (vide abstract_class_no_new_err.test.ts).
//
// ANTI-DEADLOCK (CI 2026-07-04): num runner frio/lento o sleep fixo de
// 300ms nao bastava — o connect falhava, o server ficava PRESO em
// tcp_accept eterno e o `process.wait(child)` incondicional esperava o
// server PARA SEMPRE (job "in progress" por horas; 3 processos rts
// orfaos no cleanup). Agora: connect com RETRY em janela de ~5s, e no
// caminho de falha o server e' MORTO (process.kill) em vez de aguardado
// — o teste FALHA normalmente, nunca trava a suite.
import { describe, test, expect } from "rts:test";
import { net, gc, buffer, process, thread, fs } from "rts";

function resolveRtsExe(): string {
  if (fs.exists("target/release/rts.exe") === 1) return "target/release/rts.exe";
  if (fs.exists("target/debug/rts.exe") === 1) return "target/debug/rts.exe";
  if (fs.exists("target/release/rts") === 1) return "target/release/rts";
  if (fs.exists("target/debug/rts") === 1) return "target/debug/rts";
  return "rts";
}

describe("fixture:net_tcp_echo", () => {
  test("client RTS faz echo roundtrip contra server RTS subprocess", () => {
    const exe = resolveRtsExe();
    const child = process.spawn(exe, "run\ntests/_net_echo_server.ts");

    // Server precisa de tempo pra bind+listen: tenta conectar em janela de
    // ~5s (25 x 200ms) em vez de um sleep fixo — cobre runner frio de CI.
    let client: i64 = 0;
    for (let attempt = 0; attempt < 25 && client == 0; attempt++) {
      thread.sleep_ms(200);
      client = net.tcp_connect("127.0.0.1:45123");
    }

    if (client == 0) {
      // Server nunca aceitou: mata o subprocess (preso no accept) e falha o
      // teste sem deadlock — `process.wait` aqui esperaria para sempre.
      process.kill(child);
      expect("server nao aceitou conexao em ~5s").toBe("roundtrip ok");
    } else {
      const sent = net.tcp_send(client, "rts-echo");

      const rxbuf = buffer.alloc_zeroed(8);
      const got = net.tcp_recv(client, buffer.ptr(rxbuf), 8);
      const echoed = buffer.to_string(rxbuf);
      buffer.free(rxbuf);
      net.tcp_close(client);

      // Roundtrip completo: o server ja saiu sozinho (echo + exit 0) — o
      // wait colhe o exit code sem risco de bloqueio.
      const exitCode = process.wait(child);

      expect(sent == 8 ? "1" : "0").toBe("1");
      expect(got == 8 ? "1" : "0").toBe("1");
      expect(echoed).toBe("rts-echo");
      expect(exitCode == 0 ? "1" : "0").toBe("1");
    }
  });
});
