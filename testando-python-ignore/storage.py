"""Persistência via pickle."""
import pickle
from pathlib import Path
from typing import Any


def salvar(objeto: Any, caminho: str | Path) -> None:
    """Serializa `objeto` para o arquivo `caminho` usando pickle."""
    with open(caminho, "wb") as f:
        pickle.dump(objeto, f, protocol=pickle.HIGHEST_PROTOCOL)


def carregar(caminho: str | Path) -> Any:
    """Recarrega um objeto previamente salvo com `salvar`."""
    with open(caminho, "rb") as f:
        return pickle.load(f)
