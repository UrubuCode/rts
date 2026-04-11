import { io } from "rts";

// Classe aninhada para testar objetos dentro de objetos.
class Meta {
  createdAt: number;
  source: string;
}

// Classe grande com todos os tipos suportados hoje pelo RTS.
class Everything {
  // number — inteiros exatos
  intValue: number;

  // number — float fracionário
  floatValue: number;

  // number — zero
  zeroValue: number;

  // number — negativo
  negativeValue: number;

  // string simples
  label: string;

  // string vazia
  emptyStr: string;

  // string com caracteres especiais que JSON precisa escapar
  specialChars: string;

  // bool true
  flagTrue: boolean;

  // bool false
  flagFalse: boolean;

  // objeto aninhado (outra instancia de classe)
  meta: Meta;
}

function main(): void {
  const m = new Meta();
  m.createdAt = 1700000000;
  m.source = "rts-test";

  const e = new Everything();
  e.intValue = 42;
  e.floatValue = 3.14;
  e.zeroValue = 0;
  e.negativeValue = -17;
  e.label = "hello";
  e.emptyStr = "";
  e.specialChars = "quote\"backslash\\newline\ntab\t";
  e.flagTrue = true;
  e.flagFalse = false;
  e.meta = m;

  // Serializa tudo
  const wire = JSON.stringify(e);
  io.print("--- stringify ---");
  io.print(wire);

  // Roundtrip completo
  const back = JSON.parse(wire);
  io.print("--- parse + leitura ---");
  io.print("intValue=" + back.intValue);
  io.print("floatValue=" + back.floatValue);
  io.print("zeroValue=" + back.zeroValue);
  io.print("negativeValue=" + back.negativeValue);
  io.print("label=" + back.label);
  io.print("emptyStr=[" + back.emptyStr + "]");
  io.print("specialChars=" + back.specialChars);
  io.print("flagTrue=" + back.flagTrue);
  io.print("flagFalse=" + back.flagFalse);

  // Objeto aninhado: serializa/parseia meta de dentro
  io.print("--- nested ---");
  io.print("meta.createdAt=" + back.meta.createdAt);
  io.print("meta.source=" + back.meta.source);

  // Stringify de novo deve dar o mesmo resultado (idempotencia)
  const wire2 = JSON.stringify(back);
  io.print("--- idempotent roundtrip ---");
  io.print(wire2);
}
