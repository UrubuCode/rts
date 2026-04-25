// Enum em comparações e em parâmetro de fn.
import { io } from "rts";

enum Status {
    Pending,
    Active,
    Closed,
}

function describe(s: number): string {
    if (s == Status.Pending) { return "wait"; }
    if (s == Status.Active) { return "run"; }
    return "done";
}

io.print(describe(Status.Pending));
io.print(describe(Status.Active));
io.print(describe(Status.Closed));
