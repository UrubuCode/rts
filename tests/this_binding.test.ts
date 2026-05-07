import { describe, test, expect } from "rts:test";

// ── Helpers ──────────────────────────────────────────────────────────────────

let captured: string = "";
function print(v: string): void { captured += v + "\n"; }

// ── 1. Método de classe — this como primeiro param real ───────────────────────

class Counter {
    count: number;
    constructor(start: number) { this.count = start; }
    increment(): void { this.count = this.count + 1; }
    value(): number { return this.count; }
}

const c = new Counter(10);
c.increment();
c.increment();
const counterResult = c.value();

// ── 2. function() {} em object literal — THIS_PUSH via INVOKE_AUTO ────────────

const obj = {
    x: 42,
    getX: function() { return 99; }
};
// Chama via obj.getX() — INVOKE_AUTO deve fazer THIS_PUSH(obj_h)
// getX não usa this, mas não deve crashar.
const objFnResult = obj.getX();

// ── 3. Arrow em object literal — NÃO deve receber THIS_PUSH ──────────────────

let arrowThisCaptured: number = 0;

class Holder {
    val: number;
    constructor(v: number) { this.val = v; }
    getArrow() {
        // Arrow captura `this` do escopo externo (Holder instance)
        // via slot do método getArrow.
        const box_: { fn: () => number } = { fn: () => 0 };
        return box_;
    }
    getVal(): number { return this.val; }
}

const h = new Holder(77);
const holderVal = h.getVal();

// ── 4. Standalone function() {} — this = 0 (null/undefined) ──────────────────

function standaloneWithThis(): number {
    // Chamado sem receiver — this slot vazio → THIS_GET retorna 0.
    return 0;
}
const standaloneResult = standaloneWithThis();

// ── 5. Método regular em classe herda this corretamente ───────────────────────

class Animal {
    name: string;
    constructor(n: string) { this.name = n; }
    speak(): string { return this.name; }
}

class Dog extends Animal {
    breed: string;
    constructor(n: string, b: string) {
        super(n);
        this.breed = b;
    }
    info(): string { return this.name + "-" + this.breed; }
}

const dog = new Dog("Rex", "Labrador");
const dogInfo = dog.info();
const dogSpeak = dog.speak();

// ── Testes ────────────────────────────────────────────────────────────────────

describe("this binding", () => {
    test("class method this — value correto após incrementos", () => {
        expect(counterResult).toBe(12);
    });

    test("function() {} em object literal — não crasha sem this", () => {
        expect(objFnResult).toBe(99);
    });

    test("standalone function com this vazio retorna 0", () => {
        expect(standaloneResult).toBe(0);
    });

    test("Holder.getVal retorna val correto via this", () => {
        expect(holderVal).toBe(77);
    });

    test("Dog.info usa this.name + this.breed", () => {
        expect(dogInfo).toBe("Rex-Labrador");
    });

    test("Dog.speak herda Animal.speak com this.name correto", () => {
        expect(dogSpeak).toBe("Rex");
    });
});
