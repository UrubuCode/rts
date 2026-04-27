// Subprocess: bind, accept 1, echo 8 bytes, close, exit.
import { net, buffer, process } from "rts";

const server = net.tcp_listen("127.0.0.1:51237");
if (server == 0) { process.exit(2); }
const stream = net.tcp_accept(server);
if (stream == 0) { process.exit(3); }
const buf = buffer.alloc_zeroed(8);
const n = net.tcp_recv(stream, buffer.ptr(buf), 8);
if (n != 8) { process.exit(4); }
const s = buffer.to_string(buf);
net.tcp_send(stream, s);
net.tcp_close(stream);
buffer.free(buf);
net.tcp_close(server);
process.exit(0);
