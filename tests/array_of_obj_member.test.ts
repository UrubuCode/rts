// (#592) Array de objects/classes — member access via indexing.

import { describe, test, expect } from "rts:test";

// 1. Array de obj literal — caso ja funcionava antes
const users1 = [{ name: "Alice", age: 30 }, { name: "Bob", age: 40 }];
const u1Name: string = users1[0].name;
const u1Age: i64 = users1[0].age;
const u1NameVar = users1[0];
const u1NameVarName: string = u1NameVar.name;

// 2. Array de classe user — via direct indexing
class Person {
    name: string;
    age: i64;
    constructor(n: string, a: i64) { this.name = n; this.age = a; }
}
const people: Person[] = [new Person("Alice", 30), new Person("Bob", 40)];
const p0Name: string = people[0].name;
const p1Age: i64 = people[1].age;

// 3. Array de classe user — bind via const
const p = people[0];
const pName: string = p.name;
const pAge: i64 = p.age;

// 4. Loop indexado em array de classe
let totalAge: i64 = 0;
for (let i = 0; i < people.length; i++) {
    totalAge = totalAge + people[i].age;
}

// 5. Array de classe + concatenacao em template literal
const summary = people[0].name + " is " + people[0].age;

// 6. Array literal de classe inline
const ones: Person[] = [new Person("X", 1), new Person("Y", 2), new Person("Z", 3)];
const onesNames = ones[0].name + ones[1].name + ones[2].name;

// 7. Anotacao tipada {field}[]
const tagged: { id: i64; tag: string }[] = [
    { id: 1, tag: "alpha" },
    { id: 2, tag: "beta" },
];
const t0Tag: string = tagged[0].tag;
const t1Id: i64 = tagged[1].id;

// 8. Indexing dentro de fn
function getName(arr: Person[], idx: i64): string {
    return arr[idx].name;
}
const fnName: string = getName(people, 1);

describe("array_of_obj_member", () => {
    test("obj literal: arr[0].name", () => expect(u1Name).toBe("Alice"));
    test("obj literal: arr[0].age", () => expect(u1Age).toBe(30));
    test("obj literal: bind .name", () =>
        expect(u1NameVarName).toBe("Alice"));
    test("class arr: arr[0].name", () => expect(p0Name).toBe("Alice"));
    test("class arr: arr[1].age", () => expect(p1Age).toBe(40));
    test("class arr: bind .name", () => expect(pName).toBe("Alice"));
    test("class arr: bind .age", () => expect(pAge).toBe(30));
    test("class arr: indexed loop sum", () => expect(totalAge).toBe(70));
    test("class arr: template literal", () =>
        expect(summary).toBe("Alice is 30"));
    test("class arr: inline literal", () =>
        expect(onesNames).toBe("XYZ"));
    test("typed obj arr: tag", () => expect(t0Tag).toBe("alpha"));
    test("typed obj arr: id", () => expect(t1Id).toBe(2));
    test("class arr inside fn", () => expect(fnName).toBe("Bob"));
});
