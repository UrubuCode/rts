// UMA coisa: as varias FORMAS de detectar (ou nao detectar) -0, lado a lado.
// Object.is / 1/x / Math.sign / String / === / toFixed / bitwise, sobre os
// mesmos produtores de -0. Muitas rotas escondem o -0; poucas revelam.

function probe(label: string, x: number): void {
  console.log(
    label +
      " | Object.is=-0:" + Object.is(x, -0) +
      " | ===0:" + (x === 0) +
      " | 1/x:" + 1 / x +
      " | sign:" + Math.sign(x) +
      " | String:" + String(x) +
      " | bitOr0:" + (x | 0)
  );
}

// --- produtores classicos de -0 ---
probe("literal -0      ", -0);
probe("0*-1            ", 0 * -1);
probe("-1*0            ", -1 * 0);
probe("0/-1            ", 0 / -1);
probe("-0.0+-0.0       ", -0 + -0);
probe("Math.round(-0.2)", Math.round(-0.2));
probe("Math.floor(-0)  ", Math.floor(-0));
probe("Math.ceil(-0.5) ", Math.ceil(-0.5));
probe("Math.trunc(-0.5)", Math.trunc(-0.5));
probe("Math.min(0,-0)  ", Math.min(0, -0));
probe("Math.max(-0,0)  ", Math.max(-0, 0));
probe("-Math.abs(0)    ", -Math.abs(0));
probe("Math.sqrt(-0)   ", Math.sqrt(-0));
probe("(-0)**1         ", Math.pow(-0, 1));
probe("-(0)            ", -0);

// --- NAO produzem -0 (viram +0) ---
probe("-0+0            ", -0 + 0);
probe("0+0             ", 0 + 0);
probe("-0-(-0)         ", -0 - -0);
probe("Math.max(0,-0)  ", Math.max(0, -0));
probe("Math.min(-0,0)  ", Math.min(-0, 0));
probe("Math.abs(-0)    ", Math.abs(-0));
probe("(-0)**2         ", Math.pow(-0, 2));
probe("0*1             ", 0 * 1);

// --- String()/toString escondem o sinal; toFixed tambem ---
console.log("--- string rendering hides -0 ---");
console.log("String(-0)=" + String(-0));
console.log("(-0).toString()=" + (-0).toString());
console.log("(-0).toString(2)=" + (-0).toString(2));
console.log("(-0).toFixed(2)=" + (-0).toFixed(2));
console.log("(-0).toPrecision(3)=" + (-0).toPrecision(3));
console.log("(-0).toExponential(1)=" + (-0).toExponential(1));
console.log("`${-0}`=" + `${-0}`);
console.log("(-0)+''=" + (-0 + ""));
console.log("JSON.stringify(-0)=" + JSON.stringify(-0));
console.log("JSON.stringify([-0])=" + JSON.stringify([-0]));

// --- mas Object.is e 1/x revelam ---
console.log("--- detection that works ---");
console.log("Object.is(-0,-0)=" + Object.is(-0, -0));
console.log("Object.is(-0,0)=" + Object.is(-0, 0));
console.log("Object.is(0,0)=" + Object.is(0, 0));
console.log("1/-0===-Infinity=" + (1 / -0 === -Infinity));
console.log("1/0===Infinity=" + (1 / 0 === Infinity));

// --- Math.sign preserva o zero com sinal ---
console.log("--- Math.sign roundtrip ---");
console.log("Math.sign(-0)=" + Math.sign(-0));
console.log("Object.is(Math.sign(-0),-0)=" + Object.is(Math.sign(-0), -0));
console.log("Math.sign(0)=" + Math.sign(0));
console.log("Object.is(Math.sign(0),0)=" + Object.is(Math.sign(0), 0));

// --- -0 em colecoes normaliza pra +0 (SameValueZero, nao SameValue) ---
console.log("--- collection normalization ---");
console.log("Set has 0 after add -0=" + new Set([-0]).has(0));
console.log("Set size [0,-0]=" + new Set([0, -0]).size);
console.log("[-0].includes(0)=" + [-0].includes(0));
console.log("[-0].indexOf(0)=" + [-0].indexOf(0));
console.log("[0].includes(-0)=" + [0].includes(-0));
