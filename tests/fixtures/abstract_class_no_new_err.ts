// ERRO esperado: classe abstract não pode ser instanciada via new.
abstract class Shape {
    abstract area(): number;
}

const s = new Shape();
