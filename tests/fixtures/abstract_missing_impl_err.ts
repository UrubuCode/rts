// ERRO esperado: classe concreta não implementa todos os abstract.
abstract class Shape {
    abstract area(): number;
    abstract perimeter(): number;
}

class Square extends Shape {
    side: number = 5;
    area(): number { return this.side * this.side; }
    // perimeter() faltando!
}

const s = new Square();
