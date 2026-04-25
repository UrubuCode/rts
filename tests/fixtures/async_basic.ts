// async/await fase 1: síncrono — async é flag aceita, await é no-op.
import { io, gc } from "rts";

async function getValue(): Promise<number> {
    return 42;
}

async function compute(): Promise<number> {
    const x = await getValue();
    return x + 8;
}

const r = compute();
const h = gc.string_from_i64(r as number);
io.print(h); gc.string_free(h); // 50
