"""Demonstra salvar e recarregar um objeto com pickle."""
from models import Inventario, Produto
from storage import salvar, carregar

ARQUIVO = "inventario.pkl"


def criar_inventario() -> Inventario:
    inv = Inventario(nome_loja="Loja do Daniel")
    inv.adicionar(Produto("Teclado", 150.0, 10))
    inv.adicionar(Produto("Mouse", 80.0, 25))
    inv.adicionar(Produto("Monitor", 1200.0, 4))
    return inv


def main() -> None:
    # 1. Cria e salva
    original = criar_inventario()
    salvar(original, ARQUIVO)
    print(f"Salvo em {ARQUIVO}")
    print(f"  Loja: {original.nome_loja}")
    print(f"  Produtos: {len(original.produtos)}")
    print(f"  Valor do estoque: R$ {original.valor_estoque():.2f}")

    # 2. Recarrega
    recarregado = carregar(ARQUIVO)
    print(f"\nRecarregado de {ARQUIVO}")
    print(f"  Loja: {recarregado.nome_loja}")
    for p in recarregado.produtos:
        print(f"  - {p.nome}: {p.quantidade}x R$ {p.preco:.2f} = R$ {p.valor_total():.2f}")
    print(f"  Valor do estoque: R$ {recarregado.valor_estoque():.2f}")

    # 3. Confere que os dados batem
    assert original == recarregado, "Os dados recarregados diferem do original!"
    print("\nOK: objeto recarregado é idêntico ao original.")


if __name__ == "__main__":
    main()
