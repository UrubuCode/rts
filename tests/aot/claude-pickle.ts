// rts:serde under `rts compile`: a class instance and a function by reference,
// pickled and revived inside a compiled binary — which v1 could not do (its
// function registry was filled by the JIT and empty in an AOT binary).
//
// The CI step runs this under `rts run` and as a compiled binary and DIFFS the
// two outputs. The bytes are printed too, so the diff also says that both
// builds write the same stream for the same graph: a module key or a class
// name that differed between them would show up here before it showed up as a
// file one build wrote and the other could not read.
import { serialize, deserialize } from "rts:serde";

class Pessoa {
  #segredo: number;
  nome: string;
  constructor(nome: string, segredo: number) {
    this.nome = nome;
    this.#segredo = segredo;
  }
  get segredo(): number {
    return this.#segredo;
  }
  oi(): string {
    return "oi " + this.nome;
  }
}

function dobro(x: number): number {
  return x * 2;
}

const graph: any = { p: new Pessoa("ana", 7), f: dobro, m: new Map([["k", [1, 2]]]) };
graph.self = graph;
const bytes = serialize(graph);
console.log(Array.from(bytes).join(","));
const back: any = deserialize(bytes);
console.log(back.p.oi(), back.p instanceof Pessoa, back.p.segredo, back.f(21));
console.log(back.m.get("k")[1], back.self === back);
