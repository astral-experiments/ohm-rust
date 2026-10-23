use std::ffi::OsString;

use run_make_support::{bare_rustc, bare_rustdoc};

fn main() {
    #[cfg(unix)]
    let arg: OsString = std::os::unix::ffi::OsStringExt::from_vec(vec![0xff]);
    #[cfg(windows)]
    let arg: OsString = std::os::windows::ffi::OsStringExt::from_wide(&[0xd800]);

    let expected = format!("argument 1 is not valid Unicode: {arg:?}");
    for output in [
        bare_rustc().env_remove("RUST_BACKTRACE").arg(&arg).run_fail(),
        bare_rustdoc().env_remove("RUST_BACKTRACE").arg(&arg).run_fail(),
    ] {
        output.assert_exit_code(1).assert_stderr_contains(&expected);
    }
}
