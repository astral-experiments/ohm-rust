//@ run-pass

#![deny(unused_parens, unfulfilled_lint_expectations)]

// Tests that lint attributes on statements/expressions are
// correctly applied to non-builtin early (AST) lints

fn main() {
    #[allow(unused_parens)]
    {
        #[expect(unused_parens)]
        {
            let _ = (8);
        }
        let _ = (9);
    }
}
