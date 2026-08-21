//! O VOCABULÁRIO do segundo lote de propriedades: os keywords novos, o parse e a
//! serialização computada de cada um, num sítio só.
//!
//! ## O que este módulo promete, e o que NÃO promete
//!
//! Promete que a declaração deixa de ser deitada fora: é parseada, guardada no
//! campo de `ComputedStyle` e devolvida por `getComputedStyle`. **Não promete
//! geometria.** `text-overflow: ellipsis`, `-webkit-line-clamp`, `object-fit`,
//! `align-content` e as restantes só mudam a caixa quando o LAYOUT as ler, e o
//! layout não as lê hoje — o fluxo inline e o de blocos estão a ser mexidos por
//! outra gente, e escrever um consumo em cima disso seria dois motores a decidir
//! a mesma caixa.
//!
//! Cada propriedade abaixo diz, no seu comentário, qual das duas coisas é. Uma
//! propriedade "reconhecida" que não faz o que o nome dela diz é pior do que uma
//! ausente — a diferença entre as duas é estar escrito.
//!
//! ## Porquê um módulo e não mais braços em `parse.rs`/`fmt.rs`
//!
//! `parse.rs` já está em 660 linhas e `fmt.rs` em 400, ambos acima do teto de 500
//! do repositório para um ficheiro que não é codegen. Um lote de quinze
//! propriedades entra por um módulo próprio, ligado por UM braço em cada um dos
//! dois — que é a regra da casa para código novo em ficheiro já grande.

pub(in crate::style::vocab) use super::background::BgPosition;
pub(in crate::style::vocab) use super::aplica::set_if;
pub(in crate::style::vocab) use super::lengths::{parse_dimension, split_top_ws};
pub(in crate::style::vocab) use super::props::ComputedStyle;
pub(in crate::style::vocab) use super::values::{AlignItems, Dimension, JustifyContent};

mod tipos;
mod aplicar;
mod computado;

pub use tipos::*;
pub use aplicar::*;
pub use computado::*;

// Os corpos movidos dizem `super::color::…`, `super::lengths::…` e mais quatro,
// como o ficheiro único dizia. Aí `super` era `style`; aqui é `vocab`. Reimportar
// os nomes no PAI mantém-nos a resolver sem tocar numa linha do que se moveu.
use super::{color, effects, fmt_values, lengths, parse, values};
