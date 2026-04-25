// Decorator factory: @decorator(arg) recebe args em compile-time.
import { io } from "rts";

function entity(name: string): i64 {
  io.print("registrando entidade: " + name);
  return 0;
}

@entity("usuario")
class User {
  hi(): void { io.print("usuario.hi"); }
}

@entity("produto")
class Product {
  hi(): void { io.print("produto.hi"); }
}

new User().hi();
new Product().hi();
