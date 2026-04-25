// ERRO esperado: método private acessado de fora.
class C {
    private secret(): number { return 42; }
}

const c = new C();
const v = c.secret(); // erro
