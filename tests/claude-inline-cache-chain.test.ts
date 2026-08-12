import { describe, test, expect } from "rts:test";

// O que um sítio de leitura pode lembrar quando a resposta não estava no objeto
// que ele perguntou.
//
// Um método vive no protótipo, não na instância, portanto o cache que só sabia
// procurar na shape do RECETOR nunca armava: 200 000 chamadas davam 200 007
// misses. A forma indireta lembra o endereço do detentor e o tipo que ele tinha,
// e o que estes testes fixam é exatamente aquilo que torna isso seguro — cada um
// é um caso em que lembrar seria errado.

describe("um sítio que lê através da cadeia", () => {
  test("não confunde duas classes cujas instâncias têm os mesmos campos", () => {
    // O caso que decide o desenho. `A` e `B` chegam à MESMA shape, porque é
    // para isso que uma árvore de shapes serve — logo o número que o cache
    // compara seria o mesmo, e um sítio aquecido numa leria os métodos da
    // outra. O que os separa é o tipo ser cunhado contra a ligação.
    class A {
      x: number = 1;
      m(): string {
        return "A";
      }
    }
    class B {
      x: number = 1;
      m(): string {
        return "B";
      }
    }
    // UM sítio de chamada, duas classes, e a ordem importa: a terceira leitura
    // é a que falharia se a segunda tivesse sobrescrito o que a primeira
    // lembrou sem que nada notasse.
    function which(o: any): string {
      return o.m();
    }
    expect(which(new A())).toBe("A");
    expect(which(new B())).toBe("B");
    expect(which(new A())).toBe("A");
  });

  test("um getter definido depois do sítio aquecer ganha ao slot que substitui", () => {
    // Um acessor é deliberadamente mantido FORA da shape, para que o caminho
    // rápido não o encontre e devolva a função em vez de a chamar. Isso quer
    // dizer que defini-lo não muda nada que um sítio compare — então a
    // definição tem de re-tipar a célula, ou um sítio já aquecido continua a
    // ler o slot para sempre.
    class G {
      v: number = 1;
    }
    const g = new G();
    function read(o: any): any {
      return o.v;
    }
    expect(read(g)).toBe(1);
    expect(read(g)).toBe(1);

    Object.defineProperty(Object.getPrototypeOf(g), "w", {
      get(): string {
        return "getter";
      },
    });
    expect(read(g)).toBe(1);
    expect((g as any).w).toBe("getter");
  });

  test("apagar uma propriedade não funde a instância com outro layout", () => {
    // `ShapeTree::remove` reconstrói a shape a partir da raiz, portanto o que
    // volta é a shape indiscriminada que todo o objeto com aqueles campos
    // partilha. Sem re-tipar contra a mesma ligação, um `delete` devolveria a
    // instância ao layout comum e o sítio voltaria a confundir classes.
    class A {
      x: number = 1;
      m(): string {
        return "A";
      }
    }
    class B {
      x: number = 1;
      m(): string {
        return "B";
      }
    }
    function which(o: any): string {
      return o.m();
    }
    const a: any = new A();
    which(a);
    delete a.x;
    expect(which(a)).toBe("A");
    expect(which(new B())).toBe("B");
  });

  test("um proxy continua a responder pelo seu handler", () => {
    // Um proxy era incacheável POR ACIDENTE: não tem propriedades próprias, por
    // isso a procura na shape falhava. Um resolvedor autorizado a olhar para a
    // cadeia destrói esse argumento, e a recusa passa a ter de ser explícita.
    class A {
      m(): string {
        return "A";
      }
    }
    function which(o: any): string {
      return o.m();
    }
    const direto = new A();
    which(direto);
    const espelho = new Proxy(direto, {
      get(alvo: any, chave: any): any {
        return chave === "m" ? () => "P" : alvo[chave];
      },
    });
    expect(which(espelho)).toBe("P");
    expect(which(direto)).toBe("A");
  });

  test("mudar de que um objeto herda invalida o que o sítio lembrava", () => {
    // Isto é o que substitui a validity cell do V8. Não há token nem palavra
    // global: mudar a ligação muda o tipo, e o tipo é o que o caminho rápido já
    // comparava. Nada que alguém se possa esquecer de chamar.
    class A {
      m(): string {
        return "A";
      }
    }
    class B {
      m(): string {
        return "B";
      }
    }
    function which(o: any): string {
      return o.m();
    }
    const o = new A();
    expect(which(o)).toBe("A");
    Object.setPrototypeOf(o, B.prototype);
    expect(which(o)).toBe("B");
  });

  test("um callee que é propriedade própria continua correto", () => {
    // A forma indireta é escolhida pela POSIÇÃO — o callee de uma chamada — e
    // não por prova, porque nada nesta camada sabe o tipo de uma expressão. Um
    // palpite errado custa um load, nunca uma resposta errada, e isto é o que
    // diz que custa mesmo só isso.
    const handlers = {
      a(x: number): number {
        return x + 1;
      },
    };
    let total = 0;
    for (let i = 0; i < 4; i++) total = handlers.a(total);
    expect(total).toBe(4);
  });

  test("herdar de dois níveis acima continua a responder, ainda que sem cache", () => {
    // Profundidade um é tudo o que esta mudança lembra, e o que fica de fora
    // fica CORRETO e lento em vez de errado. Um método na avó é lido pelo
    // caminho lento, exatamente como era antes.
    class Base {
      m(): string {
        return "base";
      }
    }
    class Meio extends Base {}
    class Folha extends Meio {}
    function which(o: any): string {
      return o.m();
    }
    expect(which(new Folha())).toBe("base");
    expect(which(new Folha())).toBe("base");
  });
});

describe("dois elos acima", () => {
  test("um metodo na avo e lido por carregamento", () => {
    class A { m(): string { return "A"; } }
    class B extends A {}
    class C extends B {}
    function w(o: any): string { return o.m(); }
    const c = new C();
    expect(w(c)).toBe("A");
    expect(w(c)).toBe("A");
  });

  test("relinkar o elo DO MEIO invalida o sitio", () => {
    // O caso que o terceiro guard existe para apanhar, e o unico que a
    // profundidade um nao tinha: as duas comparacoes de antes continuam a
    // suceder — o recetor nao mudou e o detentor nao mudou — e a resposta
    // deixou de ser a que o recetor encontraria.
    class A { m(): string { return "A"; } }
    class B extends A {}
    class C extends B {}
    function w(o: any): string { return o.m(); }
    const c = new C();
    expect(w(c)).toBe("A");
    Object.setPrototypeOf(B.prototype, { m(): string { return "X"; } });
    expect(w(c)).toBe("X");
  });

  test("tres elos acima continua correto, e sem cache", () => {
    class A { m(): string { return "A"; } }
    class B extends A {}
    class C extends B {}
    class D extends C {}
    function w(o: any): string { return o.m(); }
    expect(w(new D())).toBe("A");
    expect(w(new D())).toBe("A");
  });
});
