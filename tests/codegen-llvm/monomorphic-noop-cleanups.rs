//@ needs-unwind
//@ only-x86_64
//@ only-64bit (the usize storage checks assume 64 bits)
//@ ignore-msvc (this optimization excludes funclet targets)
//@ compile-flags: -Copt-level=0 -Cdebuginfo=2 -Cpanic=unwind -Ccodegen-units=1
// Use v0 to keep the two generic instances in a stable order.
//@ compile-flags: -Csymbol-mangling-version=v0

#![crate_type = "lib"]

use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Tracked;

impl Drop for Tracked {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

// A zero-sized value can still need real drop glue, on both return and unwind.
// CHECK-LABEL: ; monomorphic_noop_cleanups::generic::<monomorphic_noop_cleanups::Tracked>
// CHECK: define
// CHECK: %value = alloca [0 x i8]
// CHECK: invoke void %callback()
// CHECK-NEXT: to label %{{[^ ]+}} unwind label %[[CLEANUP:[^, ]+]]
// CHECK: invoke void @[[DROP_GLUE:[^(]*drop_glue[^(]*Tracked[^(]*]](ptr %value)
// CHECK: [[CLEANUP]]:
// CHECK: landingpad { ptr, i32 }
// CHECK: call void @[[DROP_GLUE]](ptr %value)
// CHECK: resume { ptr, i32 }
// CHECK-LABEL: {{^[}]}}

// Once T is usize, neither the callback nor black_box needs a cleanup. Keep
// the named value addressable and described by debuginfo across the callback.
// CHECK-LABEL: ; monomorphic_noop_cleanups::generic::<usize>
// CHECK: define{{.*}}!dbg ![[USIZE_SCOPE:[0-9]+]] {
// CHECK: %value = alloca [8 x i8]
// CHECK: store i64 %{{[^,]+}}, ptr %value
// CHECK: #dbg_declare(ptr %value, ![[VALUE_DBG:[0-9]+]], !DIExpression()
// CHECK: call void %callback()
// CHECK-NOT: {{invoke|landingpad|resume}}
// CHECK-LABEL: {{^[}]}}

#[inline(never)]
fn generic<T>(value: T, callback: fn()) -> usize {
    callback();
    std::hint::black_box(&value);
    17
}

pub fn instantiate(callback: fn()) {
    generic(42_usize, callback);
    generic(Tracked, callback);
}

// CHECK: ![[VALUE_DBG]] = !DILocalVariable(name: "value", arg: 1, scope: ![[USIZE_SCOPE]]
