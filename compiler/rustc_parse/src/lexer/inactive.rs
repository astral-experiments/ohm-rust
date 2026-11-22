use rustc_ast::token::{self, Delimiter, Token};
use rustc_ast::tokenstream::TokenTree::{self, Delimited, Token as Tok};
use rustc_ast::tokenstream::{TokenStream, TokenStreamIter};
use rustc_lexer::{Cursor, FrontmatterAllowed};
use rustc_session::config::Cfg;
use rustc_span::{BytePos, kw, sym};

use super::Lexer;

/// Recognize a top-level array declaration with exactly one disabled feature gate.
/// Keep its declaration and cfg attribute for the parser, expansion and check-cfg.
fn disabled_static(before: &[TokenTree], cfg: &Cfg) -> bool {
    let [
        attrs @ ..,
        Tok(storage, _),
        Tok(name, _),
        Tok(colon, _),
        Delimited(_, _, Delimiter::Bracket, _),
        Tok(eq, _),
    ] = before
    else {
        return false;
    };
    if !storage.is_keyword(kw::Static)
        || !matches!(name.kind, token::Ident(..))
        || colon.kind != token::Colon
        || eq.kind != token::Eq
    {
        return false;
    }
    let attrs = if let [prefix @ .., Tok(vis, _)] = attrs
        && vis.is_keyword(kw::Pub)
    {
        prefix
    } else {
        attrs
    };
    let [previous @ .., Tok(pound, _), Delimited(_, _, Delimiter::Bracket, attr)] = attrs else {
        return false;
    };
    // Requiring an item boundary also rules out preceding macro attributes.
    if pound.kind != token::Pound
        || !matches!(previous.last(), None | Some(Tok(Token { kind: token::Semi, .. }, _)))
    {
        return false;
    }
    let mut attr = attr.iter();
    let (Some(Tok(cfg_name, _)), Some(Delimited(_, _, Delimiter::Parenthesis, args)), None) =
        (attr.next(), attr.next(), attr.next())
    else {
        return false;
    };
    cfg_name.is_keyword(sym::cfg) && enabled(args, cfg) == Some(false)
}

/// Only feature names without escapes can be compared directly to session configuration.
fn feature(name: &Token, args: &mut TokenStreamIter<'_>, cfg: &Cfg) -> Option<bool> {
    let Tok(eq, _) = args.next()? else { return None };
    let Tok(value, _) = args.next()? else { return None };
    let token::Literal(token::Lit { kind: token::Str, suffix: None, symbol }) = value.kind else {
        return None;
    };
    (name.is_keyword(sym::feature) && eq.kind == token::Eq && !symbol.as_str().contains('\\'))
        .then(|| cfg.contains(&(sym::feature, Some(symbol))))
}

/// Handle a feature or a conjunction of required and excluded features. Even
/// after a false term, reject the whole attribute if its syntax is unfamiliar.
fn enabled(args: &TokenStream, cfg: &Cfg) -> Option<bool> {
    let mut args = args.iter();
    let Tok(name, _) = args.next()? else { return None };
    if !name.is_keyword(sym::all) {
        let result = feature(name, &mut args, cfg)?;
        return args.next().is_none().then_some(result);
    }
    let Delimited(_, _, Delimiter::Parenthesis, terms) = args.next()? else { return None };
    if args.next().is_some() {
        return None;
    }
    let mut terms = terms.iter();
    let mut result = true;
    while let Some(term) = terms.next() {
        let Tok(name, _) = term else { return None };
        if name.is_keyword(sym::not) {
            let Delimited(_, _, Delimiter::Parenthesis, inner) = terms.next()? else {
                return None;
            };
            let mut inner = inner.iter();
            let Tok(name, _) = inner.next()? else { return None };
            result &= !feature(name, &mut inner, cfg)?;
            if inner.next().is_some() {
                return None;
            }
        } else {
            result &= feature(name, &mut terms, cfg)?;
        }
        if let Some(sep) = terms.next()
            && !matches!(sep, Tok(Token { kind: token::Comma, .. }, _))
        {
            return None;
        }
    }
    Some(result)
}

/// Return the closing bracket only if every token is an ordinary integer,
/// whitespace, comma, or nested array delimiter. All other syntax, including
/// comments and literals requiring diagnostics, goes through the lexer.
fn table_end(mut rest: &str, nested: bool) -> Option<&str> {
    loop {
        rest = rest.trim_ascii_start();
        if rest.starts_with(']') {
            return Some(rest);
        }
        if !nested && let Some(fields) = rest.strip_prefix('[') {
            rest = table_end(fields, true)?.strip_prefix(']')?;
        } else {
            let (prefix, digits) = if let Some(hex) = rest.strip_prefix("0x") {
                (2, hex.bytes().take_while(u8::is_ascii_hexdigit).count())
            } else {
                (0, rest.bytes().take_while(u8::is_ascii_digit).count())
            };
            if !(1..=8).contains(&digits) {
                return None;
            }
            rest = &rest[prefix + digits..];
        }
        rest = rest.trim_ascii_start();
        if rest.starts_with(']') {
            return Some(rest);
        }
        rest = rest.strip_prefix(',')?;
    }
}

impl Lexer<'_, '_> {
    /// A disabled, syntactically inert initializer needs only its original brackets.
    pub(super) fn omit_inactive_table(&mut self, before: &[TokenTree], cfg: &Cfg) {
        if !disabled_static(before, cfg) {
            return;
        }
        let input = self.cursor.as_str();
        let Some(close) = table_end(input, false) else { return };
        let skipped = input.len() - close.len();
        if skipped >= 1024 && close[1..].trim_ascii_start().starts_with(';') {
            self.pos = self.pos + BytePos(skipped as u32);
            self.cursor = Cursor::new(close, FrontmatterAllowed::No);
        }
    }
}
