import { io } from "rts";

class Message {
  type: string;
  payload: string;
  seq: number;
}

function main(): void {
  // Usar o message como um frame de protocolo wire.
  const m = new Message();
  m.type = "event";
  m.payload = "hello";
  m.seq = 1;

  const wire = JSON.stringify(m);
  io.print(wire);

  // Simular recepcao: parse + leitura de campos.
  const recv = JSON.parse(wire);
  io.print("type=" + recv.type);
  io.print("payload=" + recv.payload);
  io.print("seq=" + recv.seq);
}
