// Um módulo só de tipos pode ser importado.
//
// O que isto pina é o que o TypeScript SIGNIFICA, e não o que esta cadeia de
// compilação faz: `import type { … } from "./x"` é apagado — é a "import
// elision" da especificação, e a razão pela qual o TS pode compilar ficheiro a
// ficheiro. Nenhum runtime vê o módulo, portanto não há módulo para resolver.
//
// Sem isso, `tests/_module_types_only.ts` chegava a tempo de execução como um
// `import` a sério, e o programa parava com
// `cannot resolve module "…_module_types_only.ts" — nothing registered that
// specifier`. O módulo é carregado e compilado; o que não existe é uma entrada
// na tabela de specifiers, porque `module_publish` cria o namespace no primeiro
// export e um módulo só de tipos não tem nenhum. Essa decisão do runtime está
// certa e não é o que muda: o que estava errado era o import sobreviver.
//
// A forma que se contornava na oniwalib (kaizeve/Oniwalib#13) era acrescentar
// um export de runtime ao módulo de tipos só para o tornar registável.
import { describe, test, expect } from "rts:test";
import type { Shape, Name } from "./_module_types_only";

// Os tipos são usados como tipos — que é a única coisa que se pode fazer com
// eles — e o programa corre.
const quadrado: Shape = { side: 4 };
const nome: Name = "quadrado";

describe("a type-only module", () => {
  test("can be imported with `import type` and the program runs", () => {
    expect(quadrado.side).toBe(4);
    expect(nome).toBe("quadrado");
  });

  test("the annotation still describes the value", () => {
    const outro: Shape = { side: quadrado.side * 2 };
    expect(outro.side).toBe(8);
  });
});

// # O que continua a NÃO funcionar, e é preciso saber
//
// `import { Shape } from "./_module_types_only"` — sem a palavra `type` — ainda
// falha com a mesma mensagem. O TypeScript elide-o também, mas para saber que
// pode fazê-lo tem de ver que `Shape` só é usado como tipo em todo o ficheiro,
// que é uma análise de uso que esta cadeia não faz. `isolatedModules` existe
// precisamente porque essa análise não é local, e escrever `import type` é o
// que a especificação pede a quem compila ficheiro a ficheiro.
//
// Não está testado aqui porque um teste que afirma uma falha fixa uma decisão
// que se quer mudar; está escrito porque a próxima pessoa a ler isto vai tentar
// a forma sem `type` e merece saber porquê.
