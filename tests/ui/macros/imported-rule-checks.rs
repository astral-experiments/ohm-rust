//@ aux-build: imported-rule-checks.rs
//@ revisions: enabled disabled
//@[enabled] check-pass

#![cfg_attr(enabled, feature(macro_guard_matcher, macro_metavar_expr))]

extern crate imported_rule_checks;

// Even when an unrestricted matcher succeeds, the consumer must check
// unstable syntax in the remaining matchers and transcribers.
const _: [(); 42] = [(); imported_rule_checks::rules!([one], [])];
//[disabled]~? ERROR `guard` fragments in macro are unstable
//[disabled]~? ERROR meta-variable expressions are unstable

fn main() {}
