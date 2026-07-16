// Cross-runtime: UMA coisa — dentro de método de classe, arrow captura o `this`
// léxico do método; `function` normal tem `this` dinâmico (undefined quando
// chamada solta, em módulo/strict). Variações: arrow em método, function em
// método, self=this, bind, arrow como campo de classe, arrow em callback de map,
// closure sobre campo vs sobre local, arrow em método estático, herança.

class Box {
  value: number;
  constructor(v: number) {
    this.value = v;
  }

  // arrow criada no método: captura o this do método
  arrowReader(): () => number {
    return () => this.value;
  }

  // function normal criada no método: this é dinâmico ao chamar
  functionReader(): () => string {
    return function (this: any): string {
      return typeof this === "undefined" ? "undefined_this" : "has_this";
    };
  }

  // padrão self = this
  selfReader(): () => number {
    const self = this;
    return function (): number {
      return self.value;
    };
  }

  // bind explícito
  boundReader(): () => number {
    return function (this: Box): number {
      return this.value;
    }.bind(this);
  }

  // arrow que MUTA this.value
  arrowBumper(): () => number {
    return () => {
      this.value += 1;
      return this.value;
    };
  }

  // closure sobre LOCAL (não usa this) — independente da instância
  localReader(): () => number {
    const snapshot = this.value;
    return () => snapshot;
  }

  // arrow dentro de callback dentro de método: this atravessa 2 níveis
  mapped(): string {
    return [1, 2].map((n) => n + this.value).join(",");
  }

  // arrow aninhada em arrow
  deepArrow(): () => () => number {
    return () => () => this.value;
  }

  static staticArrow(): string {
    const f = () => (typeof this === "function" ? "ctor" : "other");
    return f();
  }
}

const b = new Box(10);
console.log("arrow=" + b.arrowReader()());
console.log("function=" + b.functionReader()());
console.log("self=" + b.selfReader()());
console.log("bound=" + b.boundReader()());
console.log("mapped=" + b.mapped());
console.log("deep_arrow=" + b.deepArrow()()());
console.log("static_arrow=" + Box.staticArrow());

// arrow acompanha mutação do campo; local NÃO
const arrowR = b.arrowReader();
const localR = b.localReader();
b.value = 20;
console.log("after_mutation arrow=" + arrowR() + " local=" + localR());

// arrow bumper muta a instância de verdade
const bump = b.arrowBumper();
console.log("bump=" + bump() + "," + bump() + " field=" + b.value);

// arrow capturada de UMA instância não migra para outra
const b2 = new Box(100);
const fromB = b.arrowReader();
console.log("instance_bound b=" + fromB() + " b2=" + b2.arrowReader()());

// campo de classe com arrow: this fixo mesmo destacado do objeto
class Field {
  n: number = 5;
  read = () => this.n;
  readFn(): number {
    return this.n;
  }
}
const f = new Field();
const detachedArrow = f.read;
console.log("field_arrow_detached=" + detachedArrow());
f.n = 6;
console.log("field_arrow_live=" + detachedArrow());

// herança: arrow em método da base vê o this da subclasse
class Base {
  tag(): () => string {
    return () => this.constructor.name;
  }
}
class Derived extends Base {}
console.log("inherit_arrow=" + new Derived().tag()());
