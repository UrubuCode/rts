import { describe, test, expect } from "rts:test";

// (#592) const users = [{name: "..."}]; users[0].name e for-of.

const users = [{ name: "Alice", age: 30 }, { name: "Bob", age: 25 }];

const u0 = users[0];
const u0_name = u0.name;
const u1 = users[1];
const u1_name = u1.name;
const u0_age = u0.age;

let captured = "";
for (const u of users) {
  captured += u.name + ":" + u.age + ";";
}

describe("array_of_objects_access", () => {
  test("users[0].name via var", () => expect(u0_name).toBe("Alice"));
  test("users[1].name direto", () => expect(u1_name).toBe("Bob"));
  test("users[0].age tipo number", () => expect(u0_age).toBe(30));
  test("for-of u.name + u.age", () => expect(captured).toBe("Alice:30;Bob:25;"));
});
