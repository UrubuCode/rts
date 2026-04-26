// hint.* — primitivos de otimizacao.
import { io, gc, hint } from "rts";

// black_box impede otimizacao da expressao constante.
const a = hint.black_box_i64(42);
const h1 = gc.string_from_i64(a); io.print(h1); gc.string_free(h1);

// spin_loop nao tem efeito visivel — apenas roda sem panic.
hint.spin_loop();
io.print("spin-ok");

// assert_unchecked com cond=true (debug e release ok).
hint.assert_unchecked(true);
io.print("assert-ok");
