use rustc_ast::token::{self, Delimiter, Lit, LitKind, Token};
use rustc_ast::tokenstream::TokenTree::{self, Delimited, Token as Tok};
use rustc_ast::util::unicode::contains_text_flow_control_chars;
use rustc_ast::{self as ast, DUMMY_NODE_ID, Expr, ExprKind};
use rustc_lexer::{Cursor, FrontmatterAllowed};
use rustc_literal_escaper::unescape_char;
use rustc_span::{BytePos, Span, Symbol, kw};
use thin_vec::{ThinVec, thin_vec};

use super::{Lexer, ModuleLexer};

impl ModuleLexer<'_> {
    /// Only a top-level const or static can own an omitted initializer.
    /// Require a one-to-one match, including the opening bracket's position.
    pub(crate) fn restore_tables(&mut self, items: &mut [Box<ast::Item>]) -> bool {
        for item in items {
            let init = match &mut item.kind {
                ast::ItemKind::Const(c) => &mut c.body,
                ast::ItemKind::Static(s) => &mut s.expr,
                _ => continue,
            };
            let Some(expr) = init else { continue };
            let array = match expr.as_mut() {
                Expr {
                    kind: ExprKind::AddrOf(ast::BorrowKind::Ref, ast::Mutability::Not, inner),
                    ..
                } => inner.as_mut(),
                other => other,
            };
            if let ExprKind::Array(rows) = &mut array.kind
                && rows.is_empty()
                && let Ok(i) =
                    self.tables.binary_search_by_key(&array.span.lo(), |(start, _)| *start)
            {
                if self.tables[i].1.is_empty() {
                    return false;
                }
                *rows = std::mem::take(&mut self.tables[i].1);
            }
        }
        self.tables.iter().all(|(_, rows)| rows.is_empty())
    }
}

/// Require a plain array or shared slice declaration with no attributes.
/// There is no enclosing macro and the module's attributes were checked by the caller.
fn declaration(mut before: &[TokenTree]) -> bool {
    if let [rest @ .., Tok(Token { kind: token::And, .. }, _)] = before {
        before = rest;
    }
    let [head @ .., Delimited(_, _, Delimiter::Bracket, _), Tok(eq, _)] = before else {
        return false;
    };
    let mut head = head;
    if eq.kind != token::Eq {
        return false;
    }
    if let [rest @ .., Tok(Token { kind: token::Lifetime(..), .. }, _)] = head {
        head = rest;
    }
    if let [rest @ .., Tok(Token { kind: token::And, .. }, _)] = head {
        head = rest;
    }
    let [prefix @ .., Tok(storage, _), Tok(name, _), Tok(colon, _)] = head else { return false };
    let mut prefix = prefix;
    if !(storage.is_keyword(kw::Static) || storage.is_keyword(kw::Const))
        || !matches!(name.kind, token::Ident(..))
        || colon.kind != token::Colon
    {
        return false;
    }
    if let [rest @ .., Tok(vis, _)] = prefix
        && vis.is_keyword(kw::Pub)
    {
        prefix = rest;
    }
    matches!(prefix.last(), None | Some(Tok(Token { kind: token::Semi, .. }, _)))
}

fn expression(kind: ExprKind, span: Span) -> Box<Expr> {
    Box::new(Expr { kind, span, id: DUMMY_NODE_ID, attrs: Default::default(), tokens: None })
}

impl<'src> Lexer<'_, 'src> {
    /// Build ordinary AST rows directly when a generated declaration needs
    /// no token capture. Keep all symbol spellings and individual leaf spans.
    #[inline(never)]
    pub(super) fn lex_module_table(&mut self, before: &[TokenTree], module: &mut ModuleLexer<'_>) {
        self.omit_inactive_table(before, module.cfg);
        let input = self.cursor.as_str();
        if input.len() < 1024 || !declaration(before) {
            return;
        }
        let Some((rows, close)) = self.literal_table(input) else { return };
        module.tables.push((self.token.span.lo(), rows));
        self.pos = self.pos + BytePos((input.len() - close.len()) as u32);
        self.last_lifetime = None;
        self.cursor = Cursor::new(close, FrontmatterAllowed::No);
    }

    /// A failed attempt does not advance the lexer. Comments, suffixes, and
    /// characters that need a diagnostic must use ordinary tokenization.
    fn literal_table(&self, input: &'src str) -> Option<(ThinVec<Box<Expr>>, &'src str)> {
        let position = |s: &str| self.pos + BytePos((input.len() - s.len()) as u32);
        let span = |a: &str, b: &str| self.mk_sp(position(a), position(b));
        let literal = |kind: LitKind, word: &str, span| {
            let lit = Lit { kind, symbol: Symbol::intern(word), suffix: None };
            expression(ExprKind::Lit(lit), span)
        };
        let character = |start: &'src str| -> Option<(Box<Expr>, &'src str)> {
            let quoted = start.strip_prefix('\'')?;
            let (text, tail) = if let Some(tail) = quoted.strip_prefix("\\''") {
                ("\\'", tail)
            } else {
                quoted.split_once('\'')?
            };
            if unescape_char(text).is_err() || contains_text_flow_control_chars(text) {
                return None;
            }
            Some((literal(token::Char, text, span(start, tail)), tail))
        };
        let mut items = ThinVec::new();
        let mut rest = input.trim_ascii_start();
        loop {
            let start = rest;
            if let Some(inner) = rest.strip_prefix('(') {
                let (a, tail) = character(inner.trim_ascii_start())?;
                let second = tail.trim_ascii_start().strip_prefix(',')?;
                let (b, tail) = character(second.trim_ascii_start())?;
                rest = tail.trim_ascii_start().strip_prefix(')')?;
                items.push(expression(ExprKind::Tup(thin_vec![a, b]), span(start, rest)));
            } else {
                let (prefix, digits) = if let Some(hex) = rest.strip_prefix("0x") {
                    (2, hex.bytes().take_while(u8::is_ascii_hexdigit).count())
                } else {
                    (0, rest.bytes().take_while(u8::is_ascii_digit).count())
                };
                if !(1..=8).contains(&digits) {
                    return None;
                }
                let (word, tail) = rest.split_at(prefix + digits);
                rest = tail;
                items.push(literal(token::Integer, word, span(start, rest)));
            }
            rest = rest.trim_ascii_start();
            if rest.starts_with(']') {
                break;
            }
            rest = rest.strip_prefix(',')?.trim_ascii_start();
            if rest.starts_with(']') {
                break;
            }
        }
        (items.len() >= 32 && rest[1..].trim_ascii_start().starts_with(';'))
            .then_some((items, rest))
    }
}
