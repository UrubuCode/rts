import { buffer, global, io, promise, task } from "rts";

global.set("runtime.name", "rts");
const runtimeName = global.get("runtime.name");
io.print("global=" + runtimeName);

const handle = buffer.alloc(8);
buffer.write_text(handle, "hello", 0);
io.print("buffer=" + buffer.read_text(handle, 0, 5));

const pending = task.hash_sha256(runtimeName);
io.print("promise=" + promise.status(pending));
io.print("hash=" + promise.await(pending));
