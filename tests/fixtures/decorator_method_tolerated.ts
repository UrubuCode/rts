// Method/property/param decorators sao tolerados sintaticamente.
// Nao sao executados em runtime (limitacao MVP), mas o codigo
// compila e roda normalmente.
import { io } from "rts";

function noop(target: i64, key: string, desc: i64): i64 { return desc; }
function obs(target: i64, key: string): void {}
function inj(target: i64, key: string, idx: i64): void {}

class Service {
  @obs
  state: i64 = 0;

  @noop
  do(@inj dep: i64): void {
    this.state = dep;
    io.print("ok");
  }
}

const s = new Service();
s.do(7);
