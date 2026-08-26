# Arquitectura do AST CSS

## Objectivo

O motor CSS mantém agora uma separação entre a sintaxe de entrada e o IR semântico consumido pela cascade. O parser não precisa de conhecer `ComputedStyle`, layout ou o backend de renderização.

```text
CSS bruto
  → tokenize
  → StylesheetAst
  → lowering semântico
  → Stylesheet { rules, keyframes }
  → cascade
  → ComputedStyle
  → layout/pintura
```

## Tokens e source spans

`style::syntax::Token` preserva o `raw` original e um `SourceSpan` em bytes. O tokenizer reconhece whitespace, comentários, identificadores, at-keywords, hashes, strings, números, percentagens, dimensões, funções e delimitadores. Comentários permanecem disponíveis para tooling, mas são removidos apenas na serialização semântica utilizada pelo lowering.

A entrada pode ser reconstruída com `StylesheetAst::to_css()`. Essa operação não é usada para aplicar CSS; serve para inspeção e preservação da fonte.

## AST sintáctico

`StylesheetAst` contém `AstItem` recursivos:

- `QualifiedRule` contém um prelude e um `BlockAst`;
- `AtRule` contém nome, prelude e bloco opcional;
- `Invalid` preserva texto estrutural que não forma uma regra válida.

`BlockAst` preserva os `ComponentValue`, sabe se o delimitador de fecho existia e, quando contém regras aninhadas, disponibiliza um `StylesheetAst` filho. Funções e blocos delimitados não são achatados prematuramente, o que permite interpretar posteriormente `calc()`, `var()`, listas e construções CSS futuras sem alterar o tokenizer.

Uma `DeclarationAst` separa nome, valor, importância e span. O valor continua como uma lista de component values; a sua interpretação fica para a fase semântica. A grafia original do nome está em `name_raw`. `SpecifiedStyle` agrupa essas declarações e é exposto tanto para regras (`Rule::specified`) como para `style="..."` (`parse_inline_specified`).

## Lowering

`stylesheet::parse_rules` baixa `QualifiedRule` para `Rule`. Os selectors usam os tipos já existentes (`ComplexSelector`, `CompoundSelector`, `SimpleSelector` e `Combinator`). As declarações especificadas são preservadas em `Rule::source_declarations` e, em paralelo, convertidas para `RuleDecls`, que continua a ser o formato da cascade.

O lowering preserva a ordem das declarações. Isso é importante para shorthands que limpam longhands anteriores e para propriedades de timing que acumulam informação. A conversão para `ComputedStyle` continua a usar o parser semântico stateful existente, agora depois da análise estrutural. `initial` e `unset` são encaminhados como operações de defaulting: o primeiro usa os valores iniciais conhecidos e o segundo escolhe entre inicial e herança; `revert` e `revert-layer` aguardam metadados de origem e camada.

Os at-rules suportados entram no caminho estruturado: `@media` produz `MediaQuery`, `@supports` é avaliado, `@layer` é atravessado e `@keyframes` produz a tabela de animações. At-rules desconhecidos permanecem no AST para tooling, mas não afectam a cascade até existir uma implementação semântica.

## Diagnósticos

`StylesheetAst::diagnostics` e `Stylesheet::diagnostics()` reportam spans de regras inválidas, blocos sem fecho e funções/blocos sem delimitador final. Isto permite distinguir CSS não suportado de CSS malformado. Uma propriedade desconhecida continua a ser preservada no AST e pode ser observada em `Rule::source_declarations`, embora não seja aplicada ao `ComputedStyle`.

## Contrato de compatibilidade

A API pública de `Stylesheet::append_css` não mudou. A cascade continua a consultar `Stylesheet::rules`, portanto a migração não exige alterações no layout ou no bridge. O AST é adicional e pode ser usado gradualmente para implementar propriedades novas, sem reescrever o parser estrutural.

## Limites conhecidos

A representação já é recursiva e preserva a fonte, mas ainda não é um CSSOM completo. A gramática de selectors continua a ser baixada para o parser de selectors existente; propriedades CSS desconhecidas ainda não têm valores tipados; e alguns at-rules permanecem apenas como nós preservados. O próximo passo natural é introduzir parsers de valores especificados por propriedade e gerar diagnósticos semânticos sem alterar a cascade.
