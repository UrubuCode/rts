// RTS-MINE engine — núcleo Unity-like: Transform, GameObject, Scene.
//
// O padrão Unity clássico, validado contra o motor RTS atual (caps 2026-07-05):
// herança + override virtual + array polimórfico de instâncias + mutação de
// campo aninhado funcionam. Duas regras de codegen valem em TODA a engine:
//   1. retorno de chamada NUNCA é comparado inline num `if` — extrair pra const;
//   2. `let` que carrega float é anotado `: f64` (campos de classe não precisam).

/// Posição + orientação de um GameObject (câmera usa yaw/pitch).
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

/// Base de tudo que vive na cena. Subclasses sobrescrevem `start`/`update`.
/// `spriteKind`/`spriteSize`: renderer de entidade (billboard) — 0 = sem sprite;
/// 1 = slime. O entity-renderer (engine/render3d.ts) desenha quem tiver kind.
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
  /// Chamado uma vez quando o objeto entra na cena (via `scene.add`).
  start(): void {}
  /// Chamado todo frame com o delta em SEGUNDOS.
  update(dt: f64): void {}
  /// Marca para não receber mais update (a cena mantém o objeto na lista).
  destroy(): void {
    this.alive = 0;
  }
}

/// A cena: lista de GameObjects + o laço de update polimórfico.
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
    while (i < this.objects.length) {
      const o = this.objects[i];
      if (o.alive !== 0) o.update(dt);
      i = i + 1;
    }
  }
  count(): number {
    return this.objects.length;
  }
}
