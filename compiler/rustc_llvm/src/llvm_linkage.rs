/// Checks the complete component list from `llvm-config --link-shared --libs`.
pub(super) fn is_monolithic(libraries: impl IntoIterator<Item: AsRef<str>>) -> bool {
    let mut libraries = libraries.into_iter();
    let (Some(library), None) = (libraries.next(), libraries.next()) else { return false };
    let name = library.as_ref();
    name == "-lLLVM"
        || name
            .strip_prefix("-lLLVM-")
            .is_some_and(|version| version.starts_with(|c: char| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests;
