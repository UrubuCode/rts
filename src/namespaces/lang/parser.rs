use anyhow::{Result, bail};

use super::ast::{BinaryOp, Expression, Literal, UnaryOp};
use super::lexer::{Token, TokenKind};

pub fn parse_expression(tokens: Vec<Token>) -> Result<Expression> {
    let mut parser = Parser { tokens, current: 0 };
    let expression = parser.parse_nullish()?;
    if !parser.is_at_end() {
        bail!("unexpected token in expression");
    }
    Ok(expression)
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    fn parse_nullish(&mut self) -> Result<Expression> {
        let mut expression = self.parse_logical_or()?;
        while self.match_token(|kind| matches!(kind, TokenKind::QuestionQuestion)) {
            let right = self.parse_logical_or()?;
            expression = Expression::Binary {
                op: BinaryOp::NullishCoalesce,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_logical_or(&mut self) -> Result<Expression> {
        let mut expression = self.parse_logical_and()?;
        while self.match_token(|kind| matches!(kind, TokenKind::OrOr)) {
            let right = self.parse_logical_and()?;
            expression = Expression::Binary {
                op: BinaryOp::LogicalOr,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_logical_and(&mut self) -> Result<Expression> {
        let mut expression = self.parse_equality()?;
        while self.match_token(|kind| matches!(kind, TokenKind::AndAnd)) {
            let right = self.parse_equality()?;
            expression = Expression::Binary {
                op: BinaryOp::LogicalAnd,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_equality(&mut self) -> Result<Expression> {
        let mut expression = self.parse_comparison()?;
        loop {
            let op = if self.match_token(|kind| matches!(kind, TokenKind::StrictEqual)) {
                Some(BinaryOp::StrictEqual)
            } else if self.match_token(|kind| matches!(kind, TokenKind::StrictNotEqual)) {
                Some(BinaryOp::StrictNotEqual)
            } else if self.match_token(|kind| matches!(kind, TokenKind::EqualEqual)) {
                Some(BinaryOp::Equal)
            } else if self.match_token(|kind| matches!(kind, TokenKind::BangEqual)) {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_comparison()?;
            expression = Expression::Binary {
                op,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expression> {
        let mut expression = self.parse_term()?;
        loop {
            let op = if self.match_token(|kind| matches!(kind, TokenKind::GreaterThanOrEqual)) {
                Some(BinaryOp::GreaterThanOrEqual)
            } else if self.match_token(|kind| matches!(kind, TokenKind::GreaterThan)) {
                Some(BinaryOp::GreaterThan)
            } else if self.match_token(|kind| matches!(kind, TokenKind::LessThanOrEqual)) {
                Some(BinaryOp::LessThanOrEqual)
            } else if self.match_token(|kind| matches!(kind, TokenKind::LessThan)) {
                Some(BinaryOp::LessThan)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_term()?;
            expression = Expression::Binary {
                op,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_term(&mut self) -> Result<Expression> {
        let mut expression = self.parse_factor()?;
        loop {
            let op = if self.match_token(|kind| matches!(kind, TokenKind::Plus)) {
                Some(BinaryOp::Add)
            } else if self.match_token(|kind| matches!(kind, TokenKind::Minus)) {
                Some(BinaryOp::Subtract)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_factor()?;
            expression = Expression::Binary {
                op,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_factor(&mut self) -> Result<Expression> {
        let mut expression = self.parse_unary()?;
        loop {
            let op = if self.match_token(|kind| matches!(kind, TokenKind::Star)) {
                Some(BinaryOp::Multiply)
            } else if self.match_token(|kind| matches!(kind, TokenKind::Slash)) {
                Some(BinaryOp::Divide)
            } else if self.match_token(|kind| matches!(kind, TokenKind::Percent)) {
                Some(BinaryOp::Modulo)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_unary()?;
            expression = Expression::Binary {
                op,
                left: Box::new(expression),
                right: Box::new(right),
            };
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expression> {
        if self.match_token(|kind| matches!(kind, TokenKind::Bang)) {
            let value = self.parse_unary()?;
            return Ok(Expression::Unary {
                op: UnaryOp::Not,
                value: Box::new(value),
            });
        }

        if self.match_token(|kind| matches!(kind, TokenKind::Minus)) {
            let value = self.parse_unary()?;
            return Ok(Expression::Unary {
                op: UnaryOp::Negate,
                value: Box::new(value),
            });
        }

        if self.match_token(|kind| matches!(kind, TokenKind::Plus)) {
            let value = self.parse_unary()?;
            return Ok(Expression::Unary {
                op: UnaryOp::Positive,
                value: Box::new(value),
            });
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expression> {
        let mut expression = self.parse_primary()?;

        loop {
            if self.match_token(|kind| matches!(kind, TokenKind::Dot)) {
                let property = match self.advance_kind() {
                    TokenKind::Identifier(name) => name,
                    _ => bail!("expected identifier after '.'"),
                };

                expression = Expression::Member {
                    object: Box::new(expression),
                    property,
                };
                continue;
            }

            if self.match_token(|kind| matches!(kind, TokenKind::LeftParen)) {
                let mut args = Vec::new();
                if !self.check(|kind| matches!(kind, TokenKind::RightParen)) {
                    loop {
                        args.push(self.parse_nullish()?);
                        if !self.match_token(|kind| matches!(kind, TokenKind::Comma)) {
                            break;
                        }
                    }
                }
                self.consume(|kind| matches!(kind, TokenKind::RightParen), "expected ')'")?;
                expression = Expression::Call {
                    callee: Box::new(expression),
                    args,
                };
                continue;
            }

            break;
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression> {
        let token = self.advance_kind();
        let expression = match token {
            TokenKind::Number(value) => Expression::Literal(Literal::Number(value)),
            TokenKind::String(value) => Expression::Literal(Literal::String(value)),
            TokenKind::True => Expression::Literal(Literal::Bool(true)),
            TokenKind::False => Expression::Literal(Literal::Bool(false)),
            TokenKind::Null => Expression::Literal(Literal::Null),
            TokenKind::Undefined => Expression::Literal(Literal::Undefined),
            TokenKind::Identifier(name) => Expression::Identifier(name),
            TokenKind::LeftParen => {
                let inner = self.parse_nullish()?;
                self.consume(|kind| matches!(kind, TokenKind::RightParen), "expected ')'")?;
                inner
            }
            _ => bail!("unexpected token in expression"),
        };
        Ok(expression)
    }

    fn consume(
        &mut self,
        predicate: impl Fn(&TokenKind) -> bool,
        message: &str,
    ) -> Result<&TokenKind> {
        if self.check(predicate) {
            return Ok(self.advance());
        }
        bail!("{message}")
    }

    fn match_token(&mut self, predicate: impl Fn(&TokenKind) -> bool) -> bool {
        if self.check(predicate) {
            self.current += 1;
            true
        } else {
            false
        }
    }

    fn check(&self, predicate: impl Fn(&TokenKind) -> bool) -> bool {
        if self.is_at_end() {
            return false;
        }
        predicate(&self.tokens[self.current].kind)
    }

    fn advance_kind(&mut self) -> TokenKind {
        self.advance().clone()
    }

    fn advance(&mut self) -> &TokenKind {
        if !self.is_at_end() {
            self.current += 1;
        }
        &self.tokens[self.current - 1].kind
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
            || matches!(self.tokens[self.current].kind, TokenKind::Eof)
    }
}
