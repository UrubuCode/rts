// Cross-runtime: UMA coisa — closure criada dentro de try/catch/finally captura o
// binding do escopo normalmente, e o `finally` roda ANTES do retorno ser entregue,
// então mutações no finally SÃO vistas pela closure retornada. Variações: closure
// criada no try e mutada no finally; closure sobre binding de catch; finally que
// sobrescreve o valor; closure retornada do finally; try dentro de loop.

// 1) closure criada no try, binding mutado no finally: a closure lê o valor pós-finally
function tryThenFinally(): () => string {
  let state = "try";
  try {
    return () => state;
  } finally {
    state = "finally";
  }
}
console.log("mutated_in_finally=" + tryThenFinally()());

// 2) o VALOR de retorno é fixado no try, mas o binding capturado continua vivo
function returnValueVsBinding(): string {
  let n = 1;
  const read = () => n;
  function inner(): number {
    try {
      return n;
    } finally {
      n = 100;
    }
  }
  const returned = inner();
  return "returned=" + returned + " closure=" + read();
}
console.log(returnValueVsBinding());

// 3) closure sobre o binding do CATCH (escopo próprio do parâmetro do catch)
function catchBinding(): string {
  let outer = "outer";
  let readErr: () => string = () => "none";
  try {
    throw new Error("boom");
  } catch (e) {
    const msg = (e as Error).message;
    readErr = () => msg + "/" + outer;
  }
  outer = "changed";
  return readErr();
}
console.log("catch_binding=" + catchBinding());

// 4) catch param faz shadow de variável externa
function catchShadow(): string {
  let e = "outer_e";
  let read: () => string = () => "x";
  try {
    throw "thrown_e";
  } catch (e) {
    read = () => String(e);
  }
  return read() + ":" + e;
}
console.log("catch_shadow=" + catchShadow());

// 5) finally sobrescreve o return (return no finally vence)
function finallyWins(): string {
  let tag = "a";
  try {
    tag = "b";
    return "from_try:" + tag;
  } finally {
    // não retorna aqui; só muta
    tag = "c";
  }
}
console.log("finally_no_return=" + finallyWins());

// 6) closure criada DENTRO do finally
function madeInFinally(): () => string {
  let v = "start";
  let f: () => string = () => "unset";
  try {
    v = "in_try";
  } finally {
    f = () => v;
    v = "in_finally";
  }
  return f;
}
console.log("made_in_finally=" + madeInFinally()());

// 7) try/finally dentro de loop com let por-iteração
const fns: Array<() => string> = [];
for (let i = 0; i < 3; i++) {
  try {
    if (i === 1) throw new Error("skip");
    fns.push(() => "ok" + i);
  } catch {
    fns.push(() => "err" + i);
  } finally {
    // mutação por-iteração não afeta as outras voltas
  }
}
console.log("loop_try=" + fns[0]() + "," + fns[1]() + "," + fns[2]());

// 8) closures acumuladas em try, exceção no meio, finally coleta
function partial(): string {
  const acc: string[] = [];
  let count = 0;
  try {
    for (const step of ["a", "b", "c"]) {
      if (step === "c") throw new Error("stop");
      count += 1;
      acc.push(step);
    }
  } catch {
    acc.push("caught");
  } finally {
    acc.push("count=" + count);
  }
  return acc.join(",");
}
console.log("partial=" + partial());
