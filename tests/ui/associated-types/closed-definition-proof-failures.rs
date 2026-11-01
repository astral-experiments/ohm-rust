//@ revisions: current next
//@[next] compile-flags: -Znext-solver
//@ ignore-compare-mode-next-solver (explicit revisions)

#![allow(dead_code)]

trait Required {}
trait RequiredStorage { type Value: Required; }
struct BoundA;
struct BoundB;
impl RequiredStorage for BoundA {
    type Value = u8;
    //~^ ERROR the trait bound `u8: Required` is not satisfied
}
impl RequiredStorage for BoundB {
    type Value = u8;
    //~^ ERROR the trait bound `u8: Required` is not satisfied
}

// A Sized proof does not establish the nominal definition's own WF requirements.
struct NotCopy;
struct NeedsCopy<T: Copy>(T);
trait SizedStorage { type Value: Sized; }
struct WfA;
struct WfB;
impl SizedStorage for WfA {
    type Value = NeedsCopy<NotCopy>;
    //~^ ERROR the trait bound `NotCopy: Copy` is not satisfied
}
impl SizedStorage for WfB {
    type Value = NeedsCopy<NotCopy>;
    //~^ ERROR the trait bound `NotCopy: Copy` is not satisfied
}

// Identical RHS types cannot share a proof whose bound mentions naked Self.
trait For<T> {}
trait SelfDependent: Sized { type Value: For<Self>; }
struct SelfA;
struct SelfB;
impl For<SelfA> for u8 {}
impl SelfDependent for SelfA { type Value = u8; }
impl SelfDependent for SelfB {
    type Value = u8;
    //~^ ERROR the trait bound `u8: For<SelfB>` is not satisfied
}

// Nor can they share a proof that still depends on another associated projection.
trait ProjectionDependent {
    type Other;
    type Value: For<Self::Other>;
}
struct Good;
struct Bad;
struct Value;
struct ProjectionA;
struct ProjectionB;
impl For<Good> for Value {}
impl ProjectionDependent for ProjectionA {
    type Other = Good;
    type Value = Value;
}
impl ProjectionDependent for ProjectionB {
    type Other = Bad;
    type Value = Value;
    //~^ ERROR the trait bound `Value: For<Bad>` is not satisfied
}

// Nominal definitions can require region checks even when the RHS contains no regions.
struct NeedsStatic<T>(T) where for<'a> &'a T: 'static;
struct WfLifetime;
impl SizedStorage for WfLifetime {
    type Value = NeedsStatic<()>;
    //~^ ERROR the type `&'a ()` does not fulfill the required lifetime
}

fn main() {}
