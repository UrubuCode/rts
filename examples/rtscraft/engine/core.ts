// RTS-MINE engine — núcleo Unity-like: Transform, GameObject, Scene.

export class Transform {
  x: f64;
  y: f64;
  z: f64;
  yaw: f64;
  pitch: f64;
  constructor() {
    this.x = 0.0;
    this.y = 0.0;
    this.z = 0.0;
    this.yaw = 0.0;
    this.pitch = 0.0;
  }
}

export class GameObject {
  name: string;
  transform: Transform;
  alive: number;
  spriteKind: number;
  spriteSize: f64;
  constructor(name: string) {
    this.name = name;
    this.transform = new Transform();
    this.alive = 1;
    this.spriteKind = 0;
    this.spriteSize = 0.0;
  }
  start(): void {}
  update(dt: f64): void {}
  destroy(): void {
    this.alive = 0;
  }
}

export class Scene {
  objects: GameObject[];
  constructor() {
    this.objects = [];
  }

  add(go: GameObject): GameObject {
    this.objects.push(go);
    go.start();
    return go;
  }

  update(dt: f64): void {
    let i = 0;
    let write = 0;
    const limit = this.objects.length;
    while (i < limit) {
      const o = this.objects[i];
      if (o.alive !== 0) {
        o.update(dt);
        if (o.alive !== 0) {
          this.objects[write] = o;
          write = write + 1;
        }
      }
      i = i + 1;
    }
    // Mantém objectos adicionados durante o update para a iteração seguinte,
    // sem deixar os slots mortos acumularem. O caso normal não faz alloc.
    while (i < this.objects.length) {
      const o2 = this.objects[i];
      if (o2.alive !== 0) {
        this.objects[write] = o2;
        write = write + 1;
      }
      i = i + 1;
    }
    this.objects.length = write;
  }

  count(): number {
    return this.objects.length;
  }
}
