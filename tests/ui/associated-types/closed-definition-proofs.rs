//@ revisions: current next
//@[next] compile-flags: -Znext-solver
//@ ignore-compare-mode-next-solver (explicit revisions)
//@ check-pass

#![allow(dead_code)]

trait Storage {
    type Value: Copy + Default + PartialEq + core::fmt::Debug;
}

#[derive(Copy, Clone, Default, PartialEq, Debug)]
struct NeedsCopy<T: Copy>(T);

struct ArrayA;
struct ArrayB;
struct TupleA;
struct TupleB;
struct NestedArrayA;
struct NestedArrayB;
struct NominalA;
struct NominalB;
struct Scalar;

// The same closed definitions must satisfy these bounds for different impl Self types.
impl Storage for ArrayA { type Value = [u128; 4]; }
impl Storage for ArrayB { type Value = [u128; 4]; }
impl Storage for TupleA { type Value = ([u8; 4], bool); }
impl Storage for TupleB { type Value = ([u8; 4], bool); }
impl Storage for NestedArrayA { type Value = [[u16; 2]; 3]; }
impl Storage for NestedArrayB { type Value = [[u16; 2]; 3]; }
impl Storage for NominalA { type Value = NeedsCopy<u16>; }
impl Storage for NominalB { type Value = NeedsCopy<u16>; }
impl Storage for Scalar { type Value = u128; }

fn main() {}
