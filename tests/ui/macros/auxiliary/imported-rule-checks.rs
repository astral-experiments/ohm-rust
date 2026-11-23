#![feature(macro_guard_matcher, macro_metavar_expr)]

#[macro_export]
macro_rules! rules {
    ($( [ $($item:tt)* ] ),*) => { 42 };
    (guard [$(($g:guard))*]) => { 0 };
    (count $($name:ident),*) => { ${count($name)} };
}
