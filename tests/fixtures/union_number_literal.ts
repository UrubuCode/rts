// Union de literais numericos.
import { io, gc } from "rts";

function rate(stars: 1 | 2 | 3 | 4 | 5): string {
  if (stars <= 2) return "ruim";
  if (stars == 3) return "medio";
  return "bom";
}

io.print(rate(1));
io.print(rate(3));
io.print(rate(5));
