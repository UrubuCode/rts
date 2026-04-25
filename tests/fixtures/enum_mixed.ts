// Enum misto: numeric e string convivendo (TS permite).
import { io, gc } from "rts";

enum Code {
  Ok = 200,
  NotFound = 404,
  Banner = "*** atencao ***",
}

const a = gc.string_from_i64(Code.Ok);
io.print(a); gc.string_free(a);
const b = gc.string_from_i64(Code.NotFound);
io.print(b); gc.string_free(b);
io.print(Code.Banner);
