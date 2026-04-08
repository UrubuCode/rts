import { type i8, io, process, fs } from "rts";

io.print("console example");
io.print(1 + 1 + " hello from package console");
io.print(process.arch() + " hello from print() call");
io.print("console example");