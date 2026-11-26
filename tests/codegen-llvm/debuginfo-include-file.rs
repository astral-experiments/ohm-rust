//@ compile-flags: -Cdebuginfo=line-tables-only -Copt-level=0
//@ ignore-msvc

// Instructions from include! use the included file even inside another file's scope.
#![crate_type = "lib"]

// CHECK-LABEL: define {{.*}}@value(
// CHECK: xor i32 {{.*}}, 42, !dbg ![[LOC:[0-9]+]]
// CHECK: ![[LOC]] = !DILocation(line: 1, column: 10, scope: ![[SCOPE:[0-9]+]]
// CHECK: ![[SCOPE]] = !DILexicalBlockFile(scope: {{.*}}, file: ![[FILE:[0-9]+]]
// CHECK: ![[FILE]] = !DIFile(filename: "{{.*}}debuginfo-included.inc"
#[unsafe(no_mangle)]
pub fn value(x: u32) -> u32 {
    include!("auxiliary/debuginfo-included.inc")
}
