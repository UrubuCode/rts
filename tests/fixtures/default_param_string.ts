// Default com literal string.
import { io } from "rts";

function greet(name: string = "world"): void {
    io.print(name);
}

greet();
greet("Alice");
