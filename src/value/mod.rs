use std::cmp::Ordering;

use codemap::{Span, Spanned};

use crate::{
    color::Color,
    common::{Brackets, ListSeparator, Op, QuoteKind},
    error::SassResult,
    lexer::Lexer,
    parse::Parser,
    selector::Selector,
    unit::Unit,
    utils::hex_char_for,
    {Cow, Token},
};

use css_function::is_special_function;
pub(crate) use map::SassMap;
pub(crate) use number::Number;
pub(crate) use sass_function::SassFunction;

pub(crate) mod css_function;
mod map;
mod number;
mod sass_function;

#[derive(Debug, Clone)]
pub(crate) enum Value {
    Important,
    True,
    False,
    Null,
    /// A `None` value for `Number` indicates a `NaN` value
    Dimension(Option<Number>, Unit, bool),
    List(Vec<Value>, ListSeparator, Brackets),
    Color(Box<Color>),
    String(String, QuoteKind),
    Map(SassMap),
    ArgList(Vec<Spanned<Value>>),
    /// Returned by `get-function()`
    FunctionRef(SassFunction),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Value::String(s1, ..) => match other {
                Value::String(s2, ..) => s1 == s2,
                _ => false,
            },
            Value::Dimension(Some(n), unit, _) => match other {
                Value::Dimension(Some(n2), unit2, _) => {
                    if !unit.comparable(unit2) {
                        false
                    } else if unit == unit2 {
                        n == n2
                    } else if unit == &Unit::None || unit2 == &Unit::None {
                        false
                    } else {
                        n == &n2.clone().convert(unit2, unit)
                    }
                }
                _ => false,
            },
            Value::Dimension(None, ..) => false,
            Value::List(list1, sep1, brackets1) => match other {
                Value::List(list2, sep2, brackets2) => {
                    if sep1 != sep2 || brackets1 != brackets2 || list1.len() != list2.len() {
                        false
                    } else {
                        for (a, b) in list1.iter().zip(list2) {
                            if a != b {
                                return false;
                            }
                        }
                        true
                    }
                }
                _ => false,
            },
            Value::Null => matches!(other, Value::Null),
            Value::True => matches!(other, Value::True),
            Value::False => matches!(other, Value::False),
            Value::Important => matches!(other, Value::Important),
            Value::FunctionRef(fn1) => {
                if let Value::FunctionRef(fn2) = other {
                    fn1 == fn2
                } else {
                    false
                }
            }
            Value::Map(map1) => {
                if let Value::Map(map2) = other {
                    map1 == map2
                } else {
                    false
                }
            }
            Value::Color(color1) => {
                if let Value::Color(color2) = other {
                    color1 == color2
                } else {
                    false
                }
            }
            Value::ArgList(list1) => match other {
                Value::ArgList(list2) => list1 == list2,
                Value::List(list2, ListSeparator::Comma, ..) => {
                    if list1.len() != list2.len() {
                        return false;
                    }

                    for (el1, el2) in list1.iter().zip(list2) {
                        if &el1.node != el2 {
                            return false;
                        }
                    }

                    true
                }
                _ => false,
            },
        }
    }
}

impl Eq for Value {}

fn visit_quoted_string(buf: &mut String, force_double_quote: bool, string: &str) {
    let mut has_single_quote = false;
    let mut has_double_quote = false;

    let mut buffer = String::new();

    if force_double_quote {
        buffer.push('"');
    }
    let mut iter = string.chars().peekable();
    while let Some(c) = iter.next() {
        match c {
            '\'' => {
                if force_double_quote {
                    buffer.push('\'');
                } else if has_double_quote {
                    return visit_quoted_string(buf, true, string);
                } else {
                    has_single_quote = true;
                    buffer.push('\'');
                }
            }
            '"' => {
                if force_double_quote {
                    buffer.push('\\');
                    buffer.push('"');
                } else if has_single_quote {
                    return visit_quoted_string(buf, true, string);
                } else {
                    has_double_quote = true;
                    buffer.push('"');
                }
            }
            '\x00'..='\x08' | '\x0A'..='\x1F' => {
                buffer.push('\\');
                if c as u32 > 0xF {
                    buffer.push(hex_char_for(c as u32 >> 4));
                }
                buffer.push(hex_char_for(c as u32 & 0xF));

                let next = match iter.peek() {
                    Some(v) => v,
                    None => break,
                };

                if next.is_ascii_hexdigit() || next == &' ' || next == &'\t' {
                    buffer.push(' ');
                }
            }
            '\\' => {
                buffer.push('\\');
                buffer.push('\\');
            }
            _ => buffer.push(c),
        }
    }

    if force_double_quote {
        buffer.push('"');
    } else {
        let quote = if has_double_quote { '\'' } else { '"' };
        buffer = format!("{}{}{}", quote, buffer, quote);
    }
    buf.push_str(&buffer);
}

impl Value {
    pub fn is_null(&self) -> bool {
        match self {
            Value::Null => true,
            Value::String(i, QuoteKind::None) if i.is_empty() => true,
            Value::List(v, _, Brackets::Bracketed) if v.is_empty() => false,
            Value::List(v, ..) => v.iter().map(Value::is_null).all(|f| f),
            Value::ArgList(v, ..) if v.is_empty() => false,
            Value::ArgList(v, ..) => v.iter().map(|v| v.node.is_null()).all(|f| f),
            _ => false,
        }
    }

    pub fn to_css_string(&self, span: Span, is_compressed: bool) -> SassResult<Cow<'static, str>> {
        Ok(match self {
            Value::Important => Cow::const_str("!important"),
            Value::Dimension(num, unit, _) => match unit {
                Unit::Mul(..) | Unit::Div(..) => {
                    if let Some(num) = num {
                        return Err((
                            format!(
                                "{}{} isn't a valid CSS value.",
                                num.to_string(is_compressed),
                                unit
                            ),
                            span,
                        )
                            .into());
                    }

                    return Err((format!("NaN{} isn't a valid CSS value.", unit), span).into());
                }
                _ => {
                    if let Some(num) = num {
                        Cow::owned(format!("{}{}", num.to_string(is_compressed), unit))
                    } else {
                        Cow::owned(format!("NaN{}", unit))
                    }
                }
            },
            Value::Map(..) | Value::FunctionRef(..) => {
                return Err((
                    format!("{} isn't a valid CSS value.", self.inspect(span)?),
                    span,
                )
                    .into())
            }
            Value::List(vals, sep, brackets) => match brackets {
                Brackets::None => Cow::owned(
                    vals.iter()
                        .filter(|x| !x.is_null())
                        .map(|x| x.to_css_string(span, is_compressed))
                        .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                        .join(if is_compressed {
                            sep.as_compressed_str()
                        } else {
                            sep.as_str()
                        }),
                ),
                Brackets::Bracketed => Cow::owned(format!(
                    "[{}]",
                    vals.iter()
                        .filter(|x| !x.is_null())
                        .map(|x| x.to_css_string(span, is_compressed))
                        .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                        .join(if is_compressed {
                            sep.as_compressed_str()
                        } else {
                            sep.as_str()
                        }),
                )),
            },
            Value::Color(c) => Cow::owned(c.to_string()),
            Value::String(string, QuoteKind::None) => {
                let mut after_newline = false;
                let mut buf = String::with_capacity(string.len());
                for c in string.chars() {
                    match c {
                        '\n' => {
                            buf.push(' ');
                            after_newline = true;
                        }
                        ' ' => {
                            if !after_newline {
                                buf.push(' ');
                            }
                        }
                        _ => {
                            buf.push(c);
                            after_newline = false;
                        }
                    }
                }
                Cow::owned(buf)
            }
            Value::String(string, QuoteKind::Quoted) => {
                let mut buf = String::with_capacity(string.len());
                visit_quoted_string(&mut buf, false, string);
                Cow::owned(buf)
            }
            Value::True => Cow::const_str("true"),
            Value::False => Cow::const_str("false"),
            Value::Null => Cow::const_str(""),
            Value::ArgList(args) if args.is_empty() => {
                return Err(("() isn't a valid CSS value.", span).into());
            }
            Value::ArgList(args) => Cow::owned(
                args.iter()
                    .filter(|x| !x.is_null())
                    .map(|a| a.node.to_css_string(span, is_compressed))
                    .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                    .join(if is_compressed {
                        ListSeparator::Comma.as_compressed_str()
                    } else {
                        ListSeparator::Comma.as_str()
                    }),
            ),
        })
    }

    pub fn is_true(&self) -> bool {
        !matches!(self, Value::Null | Value::False)
    }

    pub fn unquote(self) -> Self {
        match self {
            Value::String(s1, _) => Value::String(s1, QuoteKind::None),
            Value::List(v, sep, bracket) => {
                Value::List(v.into_iter().map(Value::unquote).collect(), sep, bracket)
            }
            v => v,
        }
    }

    pub const fn span(self, span: Span) -> Spanned<Self> {
        Spanned { node: self, span }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Value::Color(..) => "color",
            Value::String(..) | Value::Important => "string",
            Value::Dimension(..) => "number",
            Value::List(..) => "list",
            Value::FunctionRef(..) => "function",
            Value::ArgList(..) => "arglist",
            Value::True | Value::False => "bool",
            Value::Null => "null",
            Value::Map(..) => "map",
        }
    }

    pub fn is_color(&self) -> bool {
        matches!(self, Value::Color(..))
    }

    pub fn is_special_function(&self) -> bool {
        match self {
            Value::String(s, QuoteKind::None) => is_special_function(s),
            _ => false,
        }
    }

    pub fn bool(b: bool) -> Self {
        if b {
            Value::True
        } else {
            Value::False
        }
    }

    pub fn cmp(&self, other: &Self, span: Span, op: Op) -> SassResult<Ordering> {
        Ok(match self {
            Value::Dimension(None, ..) => todo!(),
            Value::Dimension(Some(num), unit, _) => match &other {
                Value::Dimension(None, ..) => todo!(),
                Value::Dimension(Some(num2), unit2, _) => {
                    if !unit.comparable(unit2) {
                        return Err(
                            (format!("Incompatible units {} and {}.", unit2, unit), span).into(),
                        );
                    }
                    if unit == unit2 || unit == &Unit::None || unit2 == &Unit::None {
                        num.cmp(num2)
                    } else {
                        num.cmp(&num2.clone().convert(unit2, unit))
                    }
                }
                _ => {
                    return Err((
                        format!(
                            "Undefined operation \"{} {} {}\".",
                            self.inspect(span)?,
                            op,
                            other.inspect(span)?
                        ),
                        span,
                    )
                        .into())
                }
            },
            _ => {
                return Err((
                    format!(
                        "Undefined operation \"{} {} {}\".",
                        self.inspect(span)?,
                        op,
                        other.inspect(span)?
                    ),
                    span,
                )
                    .into());
            }
        })
    }

    pub fn not_equals(&self, other: &Self) -> bool {
        match self {
            Value::String(s1, ..) => match other {
                Value::String(s2, ..) => s1 != s2,
                _ => true,
            },
            Value::Dimension(Some(n), unit, _) => match other {
                Value::Dimension(Some(n2), unit2, _) => {
                    if !unit.comparable(unit2) {
                        true
                    } else if unit == unit2 {
                        n != n2
                    } else if unit == &Unit::None || unit2 == &Unit::None {
                        true
                    } else {
                        n != &n2.clone().convert(unit2, unit)
                    }
                }
                _ => true,
            },
            Value::List(list1, sep1, brackets1) => match other {
                Value::List(list2, sep2, brackets2) => {
                    if sep1 != sep2 || brackets1 != brackets2 || list1.len() != list2.len() {
                        true
                    } else {
                        for (a, b) in list1.iter().zip(list2) {
                            if a.not_equals(b) {
                                return true;
                            }
                        }
                        false
                    }
                }
                _ => true,
            },
            s => s != other,
        }
    }

    // TODO:
    // https://github.com/sass/dart-sass/blob/d4adea7569832f10e3a26d0e420ae51640740cfb/lib/src/ast/sass/expression/list.dart#L39
    pub fn inspect(&self, span: Span) -> SassResult<Cow<'static, str>> {
        Ok(match self {
            Value::List(v, _, brackets) if v.is_empty() => match brackets {
                Brackets::None => Cow::const_str("()"),
                Brackets::Bracketed => Cow::const_str("[]"),
            },
            Value::List(v, sep, brackets) if v.len() == 1 => match brackets {
                Brackets::None => match sep {
                    ListSeparator::Space => v[0].inspect(span)?,
                    ListSeparator::Comma => Cow::owned(format!("({},)", v[0].inspect(span)?)),
                },
                Brackets::Bracketed => match sep {
                    ListSeparator::Space => Cow::owned(format!("[{}]", v[0].inspect(span)?)),
                    ListSeparator::Comma => Cow::owned(format!("[{},]", v[0].inspect(span)?)),
                },
            },
            Value::List(vals, sep, brackets) => Cow::owned(match brackets {
                Brackets::None => vals
                    .iter()
                    .map(|x| x.inspect(span))
                    .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                    .join(sep.as_str()),
                Brackets::Bracketed => format!(
                    "[{}]",
                    vals.iter()
                        .map(|x| x.inspect(span))
                        .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                        .join(sep.as_str()),
                ),
            }),
            Value::FunctionRef(f) => Cow::owned(format!("get-function(\"{}\")", f.name())),
            Value::Null => Cow::const_str("null"),
            Value::Map(map) => Cow::owned(format!(
                "({})",
                map.iter()
                    .map(|(k, v)| Ok(format!("{}: {}", k.inspect(span)?, v.inspect(span)?)))
                    .collect::<SassResult<Vec<String>>>()?
                    .join(", ")
            )),
            Value::Dimension(Some(num), unit, _) => {
                Cow::owned(format!("{}{}", num.inspect(), unit))
            }
            Value::Dimension(None, unit, ..) => Cow::owned(format!("NaN{}", unit)),
            Value::ArgList(args) if args.is_empty() => Cow::const_str("()"),
            Value::ArgList(args) if args.len() == 1 => Cow::owned(format!(
                "({},)",
                args.iter()
                    .filter(|x| !x.is_null())
                    .map(|a| a.node.inspect(span))
                    .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                    .join(", "),
            )),
            Value::ArgList(args) => Cow::owned(
                args.iter()
                    .filter(|x| !x.is_null())
                    .map(|a| a.node.inspect(span))
                    .collect::<SassResult<Vec<Cow<'static, str>>>>()?
                    .join(", "),
            ),
            Value::Important
            | Value::True
            | Value::False
            | Value::Color(..)
            | Value::String(..) => self.to_css_string(span, false)?,
        })
    }

    pub fn as_list(self) -> Vec<Value> {
        match self {
            Value::List(v, ..) => v,
            Value::Map(m) => m.as_list(),
            Value::ArgList(v) => v.into_iter().map(|val| val.node).collect(),
            v => vec![v],
        }
    }

    /// Parses `self` as a selector list, in the same manner as the
    /// `selector-parse()` function.
    ///
    /// Returns a `SassError` if `self` isn't a type that can be parsed as a
    /// selector, or if parsing fails. If `allow_parent` is `true`, this allows
    /// parent selectors. Otherwise, they're considered parse errors.
    ///
    /// `name` is the argument name. It's used for error reporting.
    pub fn to_selector(
        self,
        parser: &mut Parser,
        name: &str,
        allows_parent: bool,
    ) -> SassResult<Selector> {
        let string = match self.clone().selector_string(parser.span_before)? {
            Some(v) => v,
            None => return Err((format!("${}: {} is not a valid selector: it must be a string, a list of strings, or a list of lists of strings.", name, self.inspect(parser.span_before)?), parser.span_before).into()),
        };
        Ok(Parser {
            toks: &mut Lexer::new(
                string
                    .chars()
                    .map(|c| Token::new(parser.span_before, c))
                    .collect::<Vec<Token>>(),
            ),
            map: parser.map,
            path: parser.path,
            scopes: parser.scopes,
            global_scope: parser.global_scope,
            super_selectors: parser.super_selectors,
            span_before: parser.span_before,
            content: parser.content,
            flags: parser.flags,
            at_root: parser.at_root,
            at_root_has_selector: parser.at_root_has_selector,
            extender: parser.extender,
            content_scopes: parser.content_scopes,
            options: parser.options,
            modules: parser.modules,
            module_config: parser.module_config,
        }
        .parse_selector(allows_parent, true, String::new())?
        .0)
    }

    fn selector_string(self, span: Span) -> SassResult<Option<String>> {
        Ok(Some(match self {
            Value::String(text, ..) => text,
            Value::List(list, sep, ..) if !list.is_empty() => {
                let mut result = Vec::new();
                match sep {
                    ListSeparator::Comma => {
                        for complex in list {
                            if let Value::String(text, ..) = complex {
                                result.push(text);
                            } else if let Value::List(_, ListSeparator::Space, ..) = complex {
                                result.push(match complex.selector_string(span)? {
                                    Some(v) => v,
                                    None => return Ok(None),
                                });
                            } else {
                                return Ok(None);
                            }
                        }
                    }
                    ListSeparator::Space => {
                        for compound in list {
                            if let Value::String(text, ..) = compound {
                                result.push(text);
                            } else {
                                return Ok(None);
                            }
                        }
                    }
                }

                result.join(sep.as_str())
            }
            _ => return Ok(None),
        }))
    }

    pub fn is_quoted_string(&self) -> bool {
        matches!(self, Value::String(_, QuoteKind::Quoted))
    }
}
