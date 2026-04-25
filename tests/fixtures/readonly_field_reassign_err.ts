// ERRO esperado: tentativa de reassign em readonly fora do ctor.
class C {
    readonly x: number = 1;
    set(v: number): void { this.x = v; }
}
const c = new C();
c.set(5);
