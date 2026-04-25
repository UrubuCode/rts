// Arrow async + await dentro.
import { io, gc } from "rts";

async function fetch(): Promise<number> { return 7; }

const handler = async (n: number): Promise<number> => {
    const v = await fetch();
    return v * n;
};

const r = handler(6);
const h = gc.string_from_i64(r as number);
io.print(h); gc.string_free(h); // 42
