// ERRO esperado: A não declara #x mas tenta acessar de instância de B.
class A {
    foo(b: B): number { return b.#x; }
}
class B {
    #x: number = 5;
}
new A().foo(new B());
