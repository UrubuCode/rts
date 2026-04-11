import { io } from "rts";

// Constructor inicializa os campos via `this`, e argumentos
// vao para parametros nomeados.
class Point {
  x: number;
  y: number;

  constructor(initX: number, initY: number) {
    this.x = initX;
    this.y = initY;
  }

  sum(): number {
    return this.x + this.y;
  }
}

function main(): void {
  const p = new Point(3, 4);
  io.print(p.x);      // 3
  io.print(p.y);      // 4
  io.print(p.sum());  // 7
}
