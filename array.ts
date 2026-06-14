// Função de fábrica (conversão de tipo) - retorna string primitiva
function StringFactory(valor?: any): string {
  if (valor === undefined || valor === null) {
    return '';
  }
  
  // Se for objeto MinhaString, extrai o valor primitivo
  if (valor instanceof MinhaStringWrapper) {
    return valor.toString();
  }
  
  // Se for objeto, chama toString implicito
  if (typeof valor === 'object') {
    return StringFactory(valor.toString());
  }
  
  // Converte para string
  let resultado = '';
  const strValor = String(valor); // Única "trapaça" para simplicidade
  
  // Se quiser implementar manualmente a conversão de tipos:
  // if (typeof valor === 'number') { ... conversão manual de número para string }
  // if (typeof valor === 'boolean') { resultado = valor ? 'true' : 'false'; }
  
  return strValor;
}

// Classe construtora (objeto wrapper)
class MinhaStringWrapper {
  private valor: string;

  constructor(valor?: any) {
    if (valor === undefined || valor === null) {
      this.valor = '';
    } else if (valor instanceof MinhaStringWrapper) {
      this.valor = valor.toString();
    } else {
      this.valor = StringFactory(valor);
    }
  }

  // Métodos da string (implementados com while)
  charAt(indice: number): string {
    if (indice < 0 || indice >= this.valor.length) return '';
    
    let i = 0;
    while (i < this.valor.length) {
      if (i === indice) return this.valor[i];
      i++;
    }
    return '';
  }

  indexOf(substring: string): number {
    let i = 0;
    while (i <= this.valor.length - substring.length) {
      let encontrou = true;
      let j = 0;
      
      while (j < substring.length) {
        if (this.valor[i + j] !== substring[j]) {
          encontrou = false;
          break;
        }
        j++;
      }
      
      if (encontrou) return i;
      i++;
    }
    return -1;
  }

  lastIndexOf(substring: string): number {
    let i = this.valor.length - substring.length;
    
    while (i >= 0) {
      let encontrou = true;
      let j = 0;
      
      while (j < substring.length) {
        if (this.valor[i + j] !== substring[j]) {
          encontrou = false;
          break;
        }
        j++;
      }
      
      if (encontrou) return i;
      i--;
    }
    return -1;
  }

  includes(substring: string): boolean {
    return this.indexOf(substring) !== -1;
  }

  toUpperCase(): MinhaStringWrapper {
    let resultado = '';
    let i = 0;
    
    while (i < this.valor.length) {
      const codigo = this.valor.charCodeAt(i);
      
      if (codigo >= 97 && codigo <= 122) {
        resultado += String.fromCharCode(codigo - 32);
      } else {
        resultado += this.valor[i];
      }
      i++;
    }
    
    return new MinhaStringWrapper(resultado);
  }

  toLowerCase(): MinhaStringWrapper {
    let resultado = '';
    let i = 0;
    
    while (i < this.valor.length) {
      const codigo = this.valor.charCodeAt(i);
      
      if (codigo >= 65 && codigo <= 90) {
        resultado += String.fromCharCode(codigo + 32);
      } else {
        resultado += this.valor[i];
      }
      i++;
    }
    
    return new MinhaStringWrapper(resultado);
  }

  slice(inicio: number, fim?: number): MinhaStringWrapper {
    let start = inicio < 0 ? this.valor.length + inicio : inicio;
    let end = fim !== undefined ? fim : this.valor.length;
    end = end < 0 ? this.valor.length + end : end;
    
    start = Math.max(0, Math.min(start, this.valor.length));
    end = Math.max(0, Math.min(end, this.valor.length));
    
    let resultado = '';
    let i = start;
    
    while (i < end) {
      resultado += this.valor[i];
      i++;
    }
    
    return new MinhaStringWrapper(resultado);
  }

  replace(busca: string, substituicao: string): MinhaStringWrapper {
    const indice = this.indexOf(busca);
    if (indice === -1) return new MinhaStringWrapper(this.valor);
    
    let resultado = '';
    let i = 0;
    
    while (i < indice) {
      resultado += this.valor[i];
      i++;
    }
    
    resultado += substituicao;
    i += busca.length;
    
    while (i < this.valor.length) {
      resultado += this.valor[i];
      i++;
    }
    
    return new MinhaStringWrapper(resultado);
  }

  split(separador: string): MinhaStringWrapper[] {
    if (separador === '') {
      const resultado: MinhaStringWrapper[] = [];
      let i = 0;
      
      while (i < this.valor.length) {
        resultado.push(new MinhaStringWrapper(this.valor[i]));
        i++;
      }
      return resultado;
    }
    
    const resultado: MinhaStringWrapper[] = [];
    let inicio = 0;
    let i = 0;
    
    while (i <= this.valor.length - separador.length) {
      let encontrou = true;
      let j = 0;
      
      while (j < separador.length) {
        if (this.valor[i + j] !== separador[j]) {
          encontrou = false;
          break;
        }
        j++;
      }
      
      if (encontrou) {
        if (inicio !== i) {
          let parte = '';
          let k = inicio;
          while (k < i) {
            parte += this.valor[k];
            k++;
          }
          resultado.push(new MinhaStringWrapper(parte));
        }
        inicio = i + separador.length;
        i += separador.length;
      } else {
        i++;
      }
    }
    
    if (inicio <= this.valor.length) {
      let parte = '';
      let k = inicio;
      while (k < this.valor.length) {
        parte += this.valor[k];
        k++;
      }
      resultado.push(new MinhaStringWrapper(parte));
    }
    
    return resultado;
  }

  trim(): MinhaStringWrapper {
    let inicio = 0;
    let fim = this.valor.length - 1;
    
    while (inicio <= fim && this.valor[inicio] === ' ') {
      inicio++;
    }
    
    while (fim >= inicio && this.valor[fim] === ' ') {
      fim--;
    }
    
    let resultado = '';
    let i = inicio;
    
    while (i <= fim) {
      resultado += this.valor[i];
      i++;
    }
    
    return new MinhaStringWrapper(resultado);
  }

  length(): number {
    return this.valor.length;
  }

  toString(): string {
    return this.valor;
  }
  
  // Método valueOf para conversão automática
  valueOf(): string {
    return this.valor;
  }
}

// Sobrescrever globalmente (opcional - apenas para demonstração)
declare global {
  function String(value?: any): string;
  var String: {
    new (value?: any): MinhaStringWrapper;
    (value?: any): string;
  };
}

// Implementação final que simula o comportamento nativo
const MeuString = function(valor?: any): string | MinhaStringWrapper {
  // Se chamado com new (construtor)
  if (this instanceof MeuString) {
    return new MinhaStringWrapper(valor);
  }
  
  // Se chamado como função (conversão de tipo)
  return StringFactory(valor);
} as any;

// Adiciona propriedades estáticas (opcional)
MeuString.prototype = MinhaStringWrapper.prototype;
MeuString.fromCharCode = function(...codes: number[]): string {
  let resultado = '';
  let i = 0;
  
  while (i < codes.length) {
    resultado += String.fromCharCode(codes[i]);
    i++;
  }
  
  return resultado;
};

// Exemplos de uso
console.log("=== Como função (conversão) ===");
const str1 = MeuString(123);        // string primitiva
console.log(typeof str1);            // "string"
console.log(str1);                   // "123"

const str2 = MeuString(true);        // "true"
console.log(str2);                   // "true"

const str3 = MeuString(null);        // ""
console.log(str3);                   // ""

console.log("\n=== Como construtor (objeto wrapper) ===");
const str4 = new MeuString("Hello");
console.log(typeof str4);            // "object"
console.log(str4 instanceof MinhaStringWrapper); // true
console.log(str4.toString());        // "Hello"
console.log(str4.toUpperCase().toString()); // "HELLO"

console.log("\n=== Auto-boxing (string primitiva usa métodos) ===");
// No JavaScript real, primitivas automaticamente usam métodos do wrapper
const primitiva = "test";
console.log(primitiva.indexOf("e")); // 1

// Na nossa implementação, seria assim:
const primitiva2 = MeuString("test");
// Como é string primitiva, precisamos converter para wrapper para usar métodos
const wrapper = new MeuString(primitiva2 as string);
console.log(wrapper.indexOf("e"));   // 1

console.log("\n=== Exemplo completo ===");
const test = new MeuString("aaa");
console.log(test.indexOf("a"));      // 0
console.log(test.lastIndexOf("a"));  // 2
console.log(test.includes("a"));     // true

const str = new MeuString("Hello World");
console.log(str.split(" ")[0].toString()); // "Hello"
console.log(str.replace("World", "JS").toString()); // "Hello JS"