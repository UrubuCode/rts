class Array<T> {
  #value: T[];

  constructor(initialArray: T[] = []) {
    this.#value = [...initialArray];
  }

  // Getter para acessar uma cópia do array (evita modificação externa)
  get value(): T[] {
    return [...this.#value];
  }

  // Getter para o array original (uso interno)
  get rawValue(): T[] {
    return this.#value;
  }

  // Adicionar um ou mais itens ao final
  push(...items: T[]): number {
    return this.#value.push(...items);
  }

  // Remover e retornar o último item
  pop(): T | undefined {
    return this.#value.pop();
  }

  // Adicionar ao início
  unshift(...items: T[]): number {
    return this.#value.unshift(...items);
  }

  // Remover do início
  shift(): T | undefined {
    return this.#value.shift();
  }

  // Remover e/ou adicionar itens em qualquer posição
  splice(start: number, deleteCount?: number, ...items: T[]): T[] {
    return this.#value.splice(start, deleteCount ?? 0, ...items);
  }

  // Obter item por índice
  get(index: number): T | undefined {
    return this.#value[index];
  }

  // Atualizar item por índice
  set(index: number, item: T): void {
    if (index >= 0 && index < this.#value.length) {
      this.#value[index] = item;
    } else {
      throw new Error(`Índice ${index} fora dos limites`);
    }
  }

  // Filtrar o array
  filter(predicate: (item: T, index: number) => boolean): T[] {
    return this.#value.filter(predicate);
  }

  // Mapear o array
  map<U>(callback: (item: T, index: number) => U): U[] {
    return this.#value.map(callback);
  }

  // Reduzir o array
  reduce<U>(callback: (accumulator: U, item: T, index: number) => U, initialValue: U): U {
    return this.#value.reduce(callback, initialValue);
  }

  // ForEach para iterar
  forEach(callback: (item: T, index: number) => void): void {
    this.#value.forEach(callback);
  }

  // Encontrar um item
  find(predicate: (item: T, index: number) => boolean): T | undefined {
    return this.#value.find(predicate);
  }

  // Verificar se algum item atende à condição
  some(predicate: (item: T, index: number) => boolean): boolean {
    return this.#value.some(predicate);
  }

  // Verificar se todos os itens atendem à condição
  every(predicate: (item: T, index: number) => boolean): boolean {
    return this.#value.every(predicate);
  }

  // Ordenar o array
  sort(compareFn?: (a: T, b: T) => number): void {
    this.#value.sort(compareFn);
  }

  // Inverter o array
  reverse(): void {
    this.#value.reverse();
  }

  // Obter o tamanho do array
  get length(): number {
    return this.#value.length;
  }

  // Verificar se o array está vazio
  isEmpty(): boolean {
    return this.#value.length === 0;
  }

  // Limpar o array
  clear(): void {
    this.#value = [];
  }

  // Concatenar com outro array
  concat(otherArray: T[]): T[] {
    return [...this.#value, ...otherArray];
  }

  // Criar uma cópia do array interno
  clone(): Array<T> {
    return new Array<T>([...this.#value] as T);
  }

  // Converter para string
  toString(): string {
    return this.#value.toString();
  }

  // Converter para JSON
  toJSON(): T[] {
    return [...this.#value];
  }
}