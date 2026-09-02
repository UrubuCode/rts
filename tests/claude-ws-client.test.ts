// O cliente WebSocket contra o servidor deste mesmo crate, em loopback.
//
// Não toca a rede de propósito: uma fixture que depende de um servidor na
// internet falha por motivos que não são do motor, e um teste que às vezes
// passa não mede nada. O que isto cobre e um teste de unidade não consegue é o
// handshake de saída inteiro (pedido, 101, `Sec-WebSocket-Accept`), o
// mascaramento — que o cliente é obrigado a fazer e o servidor a recusar se
// faltar — e o caminho pelo laço de eventos nos dois sentidos.
//
// # Por que o `describe` está DENTRO de um callback
//
// Porque o `rts:test` avalia cada `test()` no momento em que o encontra, e um
// handshake WebSocket só acaba muitas voltas do laço de eventos depois — um
// `expect` escrito ao lado do `new WebSocket(...)` compara o estado inicial e
// falha contra um cliente que ainda não teve tempo de abrir. Registar o
// `describe` a partir do `'close'` é o que faz as asserções correrem quando os
// valores já existem; o relatório sai no fim do processo, e um `test()` chamado
// de dentro do laço entra nele como qualquer outro.
//
// A primeira versão disto resolvia o mesmo problema com um SUBPROCESSO, e
// custava caro por uma razão que não se adivinha: um processo `rts` extra
// acrescenta o seu runtime inteiro ao pico de memória da suite, que corre os
// ficheiros em PARALELO, e isso empurrava `generator_for_of_root.test.ts` —
// que é um canário do defeito ABERTO de raízes perdidas de
// `docs/engine/lost-roots.md`, e cujo próprio cabeçalho o descreve como
// sensível a pressão de memória ("300 000 rondas esgotam a heap"). Três
// corridas da suite com o subprocesso, três quedas desse ficheiro; sem ele,
// nenhuma, e a lista de perdidos vazia. Um processo em vez de dois não é
// arrumação: é a diferença entre esta fixture medir o cliente ws e medir a
// memória livre da máquina.
import { describe, test, expect } from "rts:test";
import { WebSocketServer, WebSocket } from "ws";

const PORT = 39187;

let readyStateAoConstruir = -1;
let readyStateAoAbrir = -1;
let servidorViu = "";
let ecoTexto = "";
let ecoBytes = -1;
let ecoPrimeiroByte = -1;
let codigoDeFecho = -1;

const server = new WebSocketServer({ port: PORT });

server.on("connection", (socket: any) => {
  socket.on("message", (data: any, isBinary: boolean) => {
    if (isBinary) {
      socket.send(data);
    } else {
      servidorViu = String(data);
      socket.send("echo:" + String(data));
    }
  });
});

const client = new WebSocket("ws://127.0.0.1:" + PORT + "/room?id=7", {
  origin: "https://example.test",
  headers: { "User-Agent": "Oniwalib/1" },
});

// CONNECTING é 0, e é o que o construtor tem de devolver — a resposta ao
// handshake ainda nem começou.
readyStateAoConstruir = client.readyState;

client.on("open", () => {
  readyStateAoAbrir = client.readyState;
  client.send("hello");
});

client.on("message", (data: any, isBinary: boolean) => {
  if (isBinary) {
    const bytes = Buffer.from(data);
    ecoBytes = bytes.length;
    ecoPrimeiroByte = bytes[0];
    client.close(1000, "done");
  } else {
    ecoTexto = String(data);
    client.send(Buffer.from([7, 8, 9]));
  }
});

client.on("error", (erro: any) => {
  server.close();
  relatar("the client failed to connect: " + erro.message);
});

client.on("close", (code: number) => {
  codigoDeFecho = code;
  server.close();
  relatar("");
});

// Rede de segurança: sem isto, um handshake que nunca completa deixa o processo
// a girar até o harness o matar, e um ficheiro morto por timeout diz muito
// menos do que um que falha com o nome da verificação que caiu.
const guarda = setTimeout(() => {
  server.close();
  client.close();
  relatar("the scenario did not finish within 8s");
}, 8000);

// Corre as asserções uma vez só — o `'close'` e a guarda podem ambos disparar.
let relatado = false;
function relatar(falha: string) {
  if (relatado) return;
  relatado = true;
  clearTimeout(guarda);

  describe("ws client", () => {
    test("connects and opens", () => {
      expect(falha).toBe("");
    });

    test("readyState goes CONNECTING then OPEN", () => {
      expect(readyStateAoConstruir).toBe(0);
      expect(readyStateAoAbrir).toBe(1);
    });

    test("the server receives what the client sent", () => {
      expect(servidorViu).toBe("hello");
    });

    test("a text message round-trips", () => {
      expect(ecoTexto).toBe("echo:hello");
    });

    test("a binary message round-trips as bytes", () => {
      expect(ecoBytes).toBe(3);
      expect(ecoPrimeiroByte).toBe(7);
    });

    // 1000 e não 1005: o par tem de ecoar o código que recebeu, e um eco vazio
    // dava "sem código" a quem tinha acabado de dizer "encerramento normal".
    test("close carries the code the client sent", () => {
      expect(codigoDeFecho).toBe(1000);
    });
  });
}
