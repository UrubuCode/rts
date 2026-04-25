// ERRO esperado: protected acessado de fora da classe e fora de descendentes.
class Base {
    protected y: number = 0;
}

const b = new Base();
const v = b.y; // erro: protected em Base, escopo top-level
