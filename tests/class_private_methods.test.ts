import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class BankAccount {
  #balance: number;
  #owner: string;

  constructor(owner: string, initial: number) {
    this.#owner = owner;
    this.#balance = initial;
  }

  #validate(amount: number): boolean {
    return amount > 0 && amount <= this.#balance;
  }

  #log(msg: string): void {
    print(`[${this.#owner}] ${msg}`);
  }

  deposit(amount: number): void {
    if (amount > 0) {
      this.#balance += amount;
      this.#log(`deposited ${amount}`);
    }
  }

  withdraw(amount: number): boolean {
    if (this.#validate(amount)) {
      this.#balance -= amount;
      this.#log(`withdrew ${amount}`);
      return true;
    }
    this.#log(`failed to withdraw ${amount}`);
    return false;
  }

  get balance(): number {
    return this.#balance;
  }
}

const acc = new BankAccount("Alice", 1000);
acc.deposit(500);
acc.withdraw(200);
acc.withdraw(2000);
print(`${acc.balance}`);

describe("class_private_methods", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "[Alice] deposited 500\n[Alice] withdrew 200\n[Alice] failed to withdraw 2000\n1300\n"
  ));
});
