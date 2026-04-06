interface Usuario {
  id: number;
  nome: string;
  email: string;
}

function cloneComReflexao<T>(obj: T): T {
  return obj;
}
