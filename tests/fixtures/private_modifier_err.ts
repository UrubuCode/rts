// ERRO esperado: tentar acessar private de fora da classe.
class C {
    private secret: number = 42;
}

const c = new C();
const x = c.secret; // erro: secret é private
