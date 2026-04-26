// alloc / dealloc + write/read via ptr.
import { io, gc, alloc, ptr } from "rts";

const p = alloc.alloc_zeroed(64, 8);
if (p == 0) {
  io.print("FAIL: alloc retornou 0");
} else {
  io.print("alloc-ok");
}

ptr.write_i64(p, 7777);
const v = ptr.read_i64(p);
const h = gc.string_from_i64(v);
io.print(h); gc.string_free(h); // 7777

alloc.dealloc(p, 64, 8);
io.print("dealloc-ok");
