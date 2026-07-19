"""Modelos de dados do projeto."""
from dataclasses import dataclass, field


@dataclass
class Produto:
    nome: str
    preco: float
    quantidade: int = 0

    def valor_total(self) -> float:
        return self.preco * self.quantidade


@dataclass
class Inventario:
    nome_loja: str
    produtos: list[Produto] = field(default_factory=list)

    def adicionar(self, produto: Produto) -> None:
        self.produtos.append(produto)

    def valor_estoque(self) -> float:
        return sum(p.valor_total() for p in self.produtos)
