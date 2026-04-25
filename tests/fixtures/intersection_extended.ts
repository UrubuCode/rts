// Intersection eh type-only (TS structural). Runtime usa o object literal.
import { io, gc } from "rts";

interface HasName { name: string; }
interface HasAge { age: i64; }

// Apenas valida que TS aceita a anotacao em type alias / declaration site.
const obj: HasName & HasAge = { name: "Mario", age: 35 };
const ageStr = gc.string_from_i64(obj.age);
io.print(obj.name + " com " + ageStr);
gc.string_free(ageStr);
