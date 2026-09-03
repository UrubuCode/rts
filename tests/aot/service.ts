import { twice } from "./util";

// A class, a private field and an `async` method — the three things an AOT
// binary answered wrongly while it shipped an empty frame table.
export class Service {
  private total = 0;

  async add(n: number): Promise<number> {
    const doubled = await Promise.resolve(twice(n));
    this.total = this.total + doubled;
    return this.total;
  }
}
