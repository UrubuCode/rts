import { io } from "rts";

const a = 0xF0;
const b = 0x0F;

io.print(`and  = ${a & b}`);
io.print(`or   = ${a | b}`);
io.print(`xor  = ${a ^ b}`);
io.print(`not  = ${~0}`);
io.print(`shl  = ${1 << 4}`);
io.print(`shr  = ${256 >> 2}`);
io.print(`ushr = ${16 >>> 2}`);
io.print(`mask = ${(0xAB >> 4) & 0xF}`);
