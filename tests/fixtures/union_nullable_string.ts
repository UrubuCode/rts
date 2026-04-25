// `string | null`: Handle nullable. Branch null → fallback.
import { io } from "rts";

function greet(name: string | null): string {
  if (name == null) return "ola desconhecido";
  return "ola " + name;
}

io.print(greet("Mario"));
io.print(greet(null));
io.print(greet("Ana"));
