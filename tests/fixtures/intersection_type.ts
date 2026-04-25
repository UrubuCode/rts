// Intersection: `A & B` aceito como anotação.
import { io } from "rts";

interface HasName { name: string; }
interface HasAge { age: number; }

function describe(p: HasName & HasAge): string {
    return "ok";
}

const obj = { name: "Alice", age: 30 };
io.print(describe(obj));
