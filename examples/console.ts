import { io, process } from "rts";
import { console } from "../packages/console";

console.time("console example");
console.log(1 + 1 + " hello from package console");
io.print(process.arch() + " hello from print() call");
console.timeEnd("console example");
