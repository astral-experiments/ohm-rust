//@ compile-flags: --crate-type=lib --emit=metadata --error-format=json --json artifacts -Zpublic-api-hash
//@ normalize-stderr: "[[:xdigit:]]{32}" -> "PUBLIC_API_HASH"
//@ build-pass
//@ no-pass-override

// The metadata artifact notification carries the versioned public API hash when requested.

fn main() {}
