use crate::common::{Scope, Symbol};
use crate::utils::deref_variable;
use crate::{Token, TokenKind};
use std::fmt::{self, Display};
use std::iter::Peekable;
use std::slice::Iter;

/// A style: `color: red`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Style {
    property: String,
    value: String,
}

impl Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {};", self.property, self.value)
    }
}

impl Style {
    pub fn from_tokens(tokens: &[Token], scope: &Scope) -> Result<Self, ()> {
        Ok(StyleParser::new(tokens, scope)?.parse())
    }
}

struct StyleParser<'a> {
    tokens: Peekable<Iter<'a, Token>>,
    scope: &'a Scope,
}

impl<'a> StyleParser<'a> {
    fn new(tokens: &'a [Token], scope: &'a Scope) -> Result<Self, ()> {
        if tokens.is_empty() {
            return Err(());
        }
        let tokens = tokens.iter().peekable();
        Ok(StyleParser { tokens, scope })
    }

    fn devour_whitespace_or_comment(&mut self) {
        while let Some(Token { kind, .. }) = self.tokens.peek() {
            match kind {
                TokenKind::Whitespace(_) | TokenKind::MultilineComment(_) => self.tokens.next(),
                _ => break,
            };
        }
    }

    fn eat_interpolation(&mut self) -> String {
        let mut val = String::new();
        while let Some(Token { kind, .. }) = self.tokens.next() {
            match &kind {
                TokenKind::Symbol(Symbol::CloseCurlyBrace) => break,
                TokenKind::Symbol(Symbol::OpenCurlyBrace) => {
                    todo!("invalid character in interpolation")
                }
                TokenKind::Variable(ref v) => val.push_str(
                    &deref_variable(v, self.scope)
                        .iter()
                        .map(|x| x.kind.to_string())
                        .collect::<String>(),
                ),
                _ => val.push_str(&kind.to_string()),
            }
        }
        val
    }

    fn parse(&mut self) -> Style {
        let mut property = String::new();
        // read property until `:`
        while let Some(Token { kind, .. }) = self.tokens.next() {
            match kind {
                TokenKind::Whitespace(_) | TokenKind::MultilineComment(_) => continue,
                TokenKind::Ident(ref s) => property.push_str(s),
                TokenKind::Interpolation => property.push_str(&self.eat_interpolation()),
                TokenKind::Symbol(Symbol::Colon) => break,
                _ => property.push_str(&kind.to_string()),
            };
        }

        self.devour_whitespace_or_comment();

        let mut value = String::new();

        // read styles
        while let Some(tok) = self.tokens.next() {
            match &tok.kind {
                TokenKind::Whitespace(_) => {
                    while let Some(Token { kind, .. }) = self.tokens.peek() {
                        match kind {
                            TokenKind::Whitespace(_) | TokenKind::MultilineComment(_) => {
                                self.tokens.next();
                                continue;
                            }
                            TokenKind::Ident(ref s) => {
                                if s == &String::from("-") {
                                    self.tokens.next();
                                    value.push('-');
                                    self.devour_whitespace_or_comment();
                                    break;
                                }
                            }
                            TokenKind::Interpolation => {
                                self.tokens.next();
                                value.push_str(&self.eat_interpolation());
                                break;
                            }
                            _ => {}
                        }
                        value.push(' ');
                        break;
                    }
                }
                TokenKind::Variable(ref v) => value.push_str(
                    &deref_variable(v, self.scope)
                        .iter()
                        .map(|x| x.kind.to_string())
                        .collect::<String>(),
                ),
                TokenKind::MultilineComment(_) => continue,
                TokenKind::Interpolation => value.push_str(&self.eat_interpolation()),
                _ => value.push_str(&tok.kind.to_string()),
            }
        }
        Style { property, value }
    }
}