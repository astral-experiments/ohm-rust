use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Expr, Lit, Meta, Token};

use super::{
    RustfmtOutput, RustfmtOutputMode, SUCCESS, Target, Verbosity, run_rustfmt,
    run_rustfmt_with_output, rustfmt_command,
};

const CACHE_VERSION: u32 = 2;
const ENABLE_ENVIRONMENT: &str = "OHM_INCREMENTAL_FMT";
const CACHE_ENVIRONMENT: &str = "OHM_INCREMENTAL_FMT_CACHE_DIR";
const TRACE_ENVIRONMENT: &str = "OHM_INCREMENTAL_FMT_TRACE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    invocation: String,
    configuration: BTreeMap<String, String>,
    packages: BTreeMap<String, BTreeMap<String, String>>,
}

struct Plan {
    cache_path: PathBuf,
    previous: Option<Snapshot>,
    current: Snapshot,
}

pub(super) fn enabled() -> bool {
    matches!(env::var(ENABLE_ENVIRONMENT).as_deref(), Ok("1" | "true"))
}

pub(super) fn format(
    targets: &BTreeSet<Target>,
    arguments: &[String],
    verbosity: Verbosity,
    manifest_path: Option<&Path>,
) -> Result<i32, io::Error> {
    if verbosity == Verbosity::Verbose || has_observable_emit_mode(arguments) {
        trace("falling back to complete formatting for observable output");
        return run_rustfmt(targets, arguments, verbosity);
    }

    let _cache_lock = match lock_cache(manifest_path) {
        Ok(lock) => lock,
        Err(error) => {
            trace(&format!("falling back to complete formatting: {error}"));
            return run_rustfmt(targets, arguments, verbosity);
        }
    };

    let plan = match Plan::prepare(targets, arguments, manifest_path) {
        Ok(plan) => plan,
        Err(error) => {
            trace(&format!("falling back to complete formatting: {error}"));
            return run_rustfmt(targets, arguments, verbosity);
        }
    };

    let changed_packages = plan.changed_packages();
    if changed_packages.is_empty() {
        trace(&format!(
            "cache hit: {} unchanged packages",
            plan.current.packages.len()
        ));
        return Ok(SUCCESS);
    }

    let changed_targets: BTreeSet<_> = targets
        .iter()
        .filter(|target| {
            target
                .package_root
                .to_str()
                .is_some_and(|root| changed_packages.contains(root))
        })
        .cloned()
        .collect();

    if changed_targets.is_empty() {
        trace("falling back to complete formatting for an ambiguous package");
        return run_rustfmt(targets, arguments, verbosity);
    }

    trace(&format!(
        "formatting {} of {} packages ({} of {} targets)",
        changed_packages.len(),
        plan.current.packages.len(),
        changed_targets.len(),
        targets.len()
    ));

    let output = run_rustfmt_with_output(
        &changed_targets,
        arguments,
        verbosity,
        RustfmtOutputMode::Inherit,
    )?;
    if output.status != SUCCESS {
        trace("retaining the previous cache after unsuccessful formatting");
        return Ok(output.status);
    }

    let check_only = arguments.iter().any(|argument| argument == "--check");
    match Snapshot::capture(targets, arguments, manifest_path) {
        Ok(updated) if plan.can_record(&updated, &changed_packages, check_only) => {
            let verification = verify_formatted_packages(&changed_targets, arguments)?;
            if verification.emitted_diagnostics {
                trace("not caching a formatter invocation that emitted diagnostics");
                return Ok(output.status);
            }
            if verification.status != SUCCESS {
                trace("not caching files changed concurrently with formatting");
                return Ok(output.status);
            }

            let verified = match Snapshot::capture(targets, arguments, manifest_path) {
                Ok(verified) if verified == updated => verified,
                Ok(_) => {
                    trace("not caching files changed concurrently with formatting");
                    return Ok(output.status);
                }
                Err(error) => {
                    trace(&format!("could not verify formatted files: {error}"));
                    return Ok(output.status);
                }
            };

            if let Err(error) = write_cache(&plan.cache_path, &verified) {
                trace(&format!("could not update the formatting cache: {error}"));
            }
        }
        Ok(_) => trace("not caching files changed concurrently with formatting"),
        Err(error) => trace(&format!("could not verify formatted files: {error}")),
    }

    Ok(output.status)
}

fn lock_cache(manifest_path: Option<&Path>) -> io::Result<File> {
    let directory = cache_directory(manifest_path)?;
    fs::create_dir_all(&directory)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn verify_formatted_packages(
    targets: &BTreeSet<Target>,
    arguments: &[String],
) -> io::Result<RustfmtOutput> {
    let check_arguments = verification_arguments(arguments);
    run_rustfmt_with_output(
        targets,
        &check_arguments,
        Verbosity::Quiet,
        RustfmtOutputMode::CaptureQuietly,
    )
}

fn verification_arguments(arguments: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(arguments.len() + 1);
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--emit" {
            if let Some(mode) = arguments.next().filter(|mode| mode.as_str() != "files") {
                normalized.push(argument.clone());
                normalized.push(mode.clone());
            }
        } else if argument != "--emit=files" {
            normalized.push(argument.clone());
        }
    }

    if !normalized.iter().any(|argument| argument == "--check") {
        normalized.push(String::from("--check"));
    }
    normalized
}

impl Plan {
    fn prepare(
        targets: &BTreeSet<Target>,
        arguments: &[String],
        manifest_path: Option<&Path>,
    ) -> io::Result<Self> {
        let current = Snapshot::capture(targets, arguments, manifest_path)?;
        let cache_directory = cache_directory(manifest_path)?;
        let cache_path = cache_directory.join(format!("{}.json", current.invocation));
        let previous = match File::open(&cache_path) {
            Ok(file) => serde_json::from_reader(file).ok(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };

        Ok(Self {
            cache_path,
            previous,
            current,
        })
    }

    fn changed_packages(&self) -> BTreeSet<&str> {
        let Some(previous) = self.previous.as_ref() else {
            return self.current.packages.keys().map(String::as_str).collect();
        };

        if previous.version != CACHE_VERSION
            || previous.invocation != self.current.invocation
            || previous.configuration != self.current.configuration
            || previous.packages.len() != self.current.packages.len()
        {
            return self.current.packages.keys().map(String::as_str).collect();
        }

        self.current
            .packages
            .iter()
            .filter_map(|(package, files)| {
                (previous.packages.get(package) != Some(files)).then_some(package.as_str())
            })
            .collect()
    }

    fn can_record(
        &self,
        updated: &Snapshot,
        changed_packages: &BTreeSet<&str>,
        check_only: bool,
    ) -> bool {
        if updated.version != CACHE_VERSION
            || updated.invocation != self.current.invocation
            || updated.configuration != self.current.configuration
            || updated.packages.len() != self.current.packages.len()
        {
            return false;
        }

        if check_only {
            return updated.packages == self.current.packages;
        }

        self.current.packages.iter().all(|(package, files)| {
            changed_packages.contains(package.as_str())
                || updated.packages.get(package) == Some(files)
        })
    }
}

impl Snapshot {
    fn capture(
        targets: &BTreeSet<Target>,
        arguments: &[String],
        manifest_path: Option<&Path>,
    ) -> io::Result<Self> {
        let package_roots: BTreeSet<_> = targets
            .iter()
            .map(|target| target.package_root.clone())
            .collect();

        if package_roots.is_empty()
            || targets
                .iter()
                .any(|target| !target.path.starts_with(&target.package_root))
        {
            return Err(unsupported("a target lies outside its package"));
        }

        let mut configuration = BTreeMap::new();
        let mut packages = BTreeMap::new();
        let current_directory = env::current_dir()?.canonicalize()?;
        let cache_directory = cache_directory(manifest_path)?;
        let cache_directory = if cache_directory.is_absolute() {
            cache_directory
        } else {
            current_directory.join(cache_directory)
        }
        .canonicalize()?;
        for root in &package_roots {
            let mut files = BTreeMap::new();
            let excluded_module_names =
                excluded_module_directory_names(root, &package_roots, &cache_directory)?;
            collect_package_files(
                root,
                root,
                &package_roots,
                &cache_directory,
                &excluded_module_names,
                &mut files,
                &mut configuration,
            )?;
            collect_ancestor_configuration(root, &mut configuration)?;
            packages.insert(path_string(root)?, files);
        }

        if targets.iter().any(|target| {
            target
                .package_root
                .to_str()
                .and_then(|root| packages.get(root))
                .is_none_or(|files| {
                    target
                        .path
                        .to_str()
                        .is_none_or(|path| !files.contains_key(path))
                })
        }) {
            return Err(unsupported("a target source is excluded from its package"));
        }

        collect_ancestor_configuration(&current_directory, &mut configuration)?;
        collect_global_configuration(&mut configuration)?;
        collect_explicit_configuration(arguments, &mut configuration)?;

        if let Some(path) = manifest_path {
            collect_configuration_file(path, &mut configuration)?;
        }

        let formatter_path = trusted_formatter()?;
        let formatter_hash = hash_file(&formatter_path)?;
        let mut invocation = Sha256::new();
        hash_value(&mut invocation, env!("CARGO_PKG_VERSION"));
        hash_value(&mut invocation, &path_string(&current_directory)?);
        hash_value(&mut invocation, &path_string(&formatter_path)?);
        hash_value(&mut invocation, &formatter_hash);

        for variable in [
            "APPDATA",
            "CARGO_HOME",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "HOME",
            "LD_LIBRARY_PATH",
            "RUSTC_BOOTSTRAP",
            "RUSTFMT_LOG",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "USERPROFILE",
            "XDG_CONFIG_HOME",
        ] {
            hash_environment(&mut invocation, variable)?;
        }

        for argument in arguments {
            hash_value(&mut invocation, argument);
        }
        for target in targets {
            hash_value(&mut invocation, &path_string(&target.path)?);
            hash_value(&mut invocation, &path_string(&target.package_root)?);
            hash_value(&mut invocation, target.edition.as_str());
            hash_value(&mut invocation, &target.kind);
        }

        Ok(Self {
            version: CACHE_VERSION,
            invocation: format!("{:x}", invocation.finalize()),
            configuration,
            packages,
        })
    }
}

fn collect_package_files(
    package_root: &Path,
    directory: &Path,
    package_roots: &BTreeSet<PathBuf>,
    cache_directory: &Path,
    excluded_module_names: &BTreeSet<String>,
    sources: &mut BTreeMap<String, String>,
    configuration: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if path == cache_directory {
            continue;
        }

        if file_type.is_symlink() {
            let target_type = fs::metadata(&path)?.file_type();
            if target_type.is_dir() || path.extension().is_some_and(|extension| extension == "rs") {
                return Err(unsupported("a package contains a symlinked Rust source"));
            }
        }

        if file_type.is_dir() {
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | ".hg" | ".svn"))
                || (directory == package_root && name.to_str() == Some("target"))
            {
                continue;
            }

            if path != package_root && package_roots.contains(&path) {
                continue;
            }

            collect_package_files(
                package_root,
                &path,
                package_roots,
                cache_directory,
                excluded_module_names,
                sources,
                configuration,
            )?;
            continue;
        }

        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }

        if path.extension().is_some_and(|extension| extension == "rs") {
            let contents = fs::read(&path)?;
            validate_module_paths_with_exclusions(
                &path,
                &contents,
                package_root,
                package_roots,
                cache_directory,
                excluded_module_names,
            )?;
            sources.insert(
                path_string(&path)?,
                format!("{:x}", Sha256::digest(&contents)),
            );
        } else if is_configuration_file(&path) {
            collect_configuration_file(&path, configuration)?;
        }
    }

    Ok(())
}

#[cfg(test)]
fn validate_module_paths(
    source_path: &Path,
    contents: &[u8],
    package_root: &Path,
    package_roots: &BTreeSet<PathBuf>,
    cache_directory: &Path,
) -> io::Result<()> {
    let excluded_module_names =
        excluded_module_directory_names(package_root, package_roots, cache_directory)?;
    validate_module_paths_with_exclusions(
        source_path,
        contents,
        package_root,
        package_roots,
        cache_directory,
        &excluded_module_names,
    )
}

fn validate_module_paths_with_exclusions(
    source_path: &Path,
    contents: &[u8],
    package_root: &Path,
    package_roots: &BTreeSet<PathBuf>,
    cache_directory: &Path,
    excluded_module_names: &BTreeSet<String>,
) -> io::Result<()> {
    let source = std::str::from_utf8(contents)
        .map_err(|_| unsupported("a Rust source is not valid UTF-8"))?;
    let inspect_implicit_modules = has_implicit_excluded_module(source, excluded_module_names);
    if !has_module_resolution_syntax(source) && !inspect_implicit_modules {
        return Ok(());
    }

    let syntax = syn::parse_file(source)
        .map_err(|_| unsupported("a Rust source uses syntax that cannot be safely inspected"))?;
    let mut visitor = ModulePathVisitor {
        source_path,
        package_root,
        package_roots,
        cache_directory,
        excluded_module_names: excluded_module_names.clone(),
        inspect_implicit_modules,
        inline_modules: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    match visitor.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn excluded_module_directory_names(
    package_root: &Path,
    package_roots: &BTreeSet<PathBuf>,
    cache_directory: &Path,
) -> io::Result<BTreeSet<String>> {
    let mut directories = vec![package_root.join("target")];
    if cache_directory.starts_with(package_root) {
        directories.push(cache_directory.to_owned());
    }
    directories.extend(
        package_roots
            .iter()
            .filter(|root| root.as_path() != package_root && root.starts_with(package_root))
            .cloned(),
    );

    let mut names = BTreeSet::new();
    for directory in directories {
        match fs::metadata(&directory) {
            Ok(metadata) if metadata.is_dir() => {
                if let Some(name) = directory.file_name().and_then(|name| name.to_str()) {
                    names.insert(name.to_owned());
                } else {
                    return Err(unsupported(
                        "an excluded module directory is not valid UTF-8",
                    ));
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(unsupported(
                    "an excluded module directory cannot be inspected",
                ));
            }
        }
    }

    Ok(names)
}

fn has_implicit_excluded_module(source: &str, names: &BTreeSet<String>) -> bool {
    if names.is_empty() {
        return false;
    }

    let bytes = source.as_bytes();
    for (index, _) in source.match_indices("mod") {
        if index > 0 && is_identifier_byte(bytes[index - 1]) {
            continue;
        }

        let mut cursor = index + b"mod".len();
        if bytes
            .get(cursor)
            .is_some_and(|byte| is_identifier_byte(*byte))
        {
            continue;
        }
        if skip_rust_trivia(bytes, &mut cursor).is_err() {
            return true;
        }
        if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"r#") {
            cursor += 2;
        }

        let Some(remaining) = source.get(cursor..) else {
            return true;
        };
        let identifier = remaining
            .chars()
            .take_while(|character| *character == '_' || character.is_alphanumeric())
            .collect::<String>();
        if names
            .iter()
            .any(|name| module_names_match(&identifier, name))
        {
            return true;
        }
    }

    false
}

fn module_names_match(identifier: &str, directory: &str) -> bool {
    identifier.eq_ignore_ascii_case(directory)
        || (!identifier.is_ascii()
            && !directory.is_ascii()
            && identifier.to_lowercase() == directory.to_lowercase())
}

fn has_module_resolution_syntax(source: &str) -> bool {
    let bytes = source.as_bytes();

    if source.contains("path") && source.contains('#') {
        for (index, _) in source.match_indices('#') {
            let mut cursor = index + 1;
            if skip_rust_trivia(bytes, &mut cursor).is_err() {
                return true;
            }
            if bytes.get(cursor) != Some(&b'[') {
                continue;
            }
            cursor += 1;
            if skip_rust_trivia(bytes, &mut cursor).is_err() {
                return true;
            }

            if bytes.get(cursor..cursor.saturating_add(2)) == Some(b"r#") {
                cursor += 2;
            }

            if has_identifier(bytes, cursor, b"path") {
                return true;
            }
            if has_identifier(bytes, cursor, b"cfg_attr")
                && attribute_contains_path(bytes, cursor + b"cfg_attr".len())
            {
                return true;
            }
        }
    }

    if source.contains("include") && source.contains('!') {
        for (index, _) in source.match_indices("include") {
            if index > 0 && is_identifier_byte(bytes[index - 1]) {
                continue;
            }
            let mut cursor = index + "include".len();
            if bytes
                .get(cursor)
                .is_some_and(|byte| is_identifier_byte(*byte))
            {
                continue;
            }
            if skip_rust_trivia(bytes, &mut cursor).is_err() || bytes.get(cursor) == Some(&b'!') {
                return true;
            }
        }
    }

    false
}

fn attribute_contains_path(bytes: &[u8], mut cursor: usize) -> bool {
    let mut depth = 1_usize;
    while cursor < bytes.len() {
        if skip_rust_trivia(bytes, &mut cursor).is_err() {
            return true;
        }
        let Some(byte) = bytes.get(cursor).copied() else {
            return true;
        };

        if byte == b'"' {
            cursor += 1;
            while let Some(next) = bytes.get(cursor).copied() {
                cursor += 1;
                if next == b'\\' {
                    cursor = cursor.saturating_add(1);
                } else if next == b'"' {
                    break;
                }
            }
            continue;
        }

        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            _ if has_identifier(bytes, cursor, b"path") => return true,
            _ => {}
        }
        cursor += 1;
    }

    true
}

fn has_identifier(bytes: &[u8], cursor: usize, identifier: &[u8]) -> bool {
    bytes.get(cursor..cursor.saturating_add(identifier.len())) == Some(identifier)
        && !bytes
            .get(cursor + identifier.len())
            .is_some_and(|byte| is_identifier_byte(*byte))
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_rust_trivia(bytes: &[u8], cursor: &mut usize) -> Result<(), ()> {
    loop {
        while let Some(width) = rust_whitespace_width(bytes, *cursor) {
            *cursor += width;
        }

        if bytes.get(*cursor..cursor.saturating_add(2)) == Some(b"//") {
            *cursor += 2;
            while bytes.get(*cursor).is_some_and(|byte| *byte != b'\n') {
                *cursor += 1;
            }
            continue;
        }

        if bytes.get(*cursor..cursor.saturating_add(2)) != Some(b"/*") {
            return Ok(());
        }

        *cursor += 2;
        let mut depth = 1_usize;
        while *cursor < bytes.len() {
            if bytes.get(*cursor..cursor.saturating_add(2)) == Some(b"/*") {
                depth += 1;
                *cursor += 2;
            } else if bytes.get(*cursor..cursor.saturating_add(2)) == Some(b"*/") {
                depth -= 1;
                *cursor += 2;
                if depth == 0 {
                    break;
                }
            } else {
                *cursor += 1;
            }
        }
        if depth != 0 {
            return Err(());
        }
    }
}

fn rust_whitespace_width(bytes: &[u8], cursor: usize) -> Option<usize> {
    match bytes.get(cursor).copied()? {
        b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r' | b' ' => Some(1),
        0xc2 if bytes.get(cursor + 1) == Some(&0x85) => Some(2),
        0xe2 if bytes.get(cursor + 1) == Some(&0x80)
            && matches!(
                bytes.get(cursor + 2).copied(),
                Some(0x8e | 0x8f | 0xa8 | 0xa9)
            ) =>
        {
            Some(3)
        }
        _ => None,
    }
}

struct ModulePathVisitor<'a> {
    source_path: &'a Path,
    package_root: &'a Path,
    package_roots: &'a BTreeSet<PathBuf>,
    cache_directory: &'a Path,
    excluded_module_names: BTreeSet<String>,
    inspect_implicit_modules: bool,
    inline_modules: Vec<String>,
    error: Option<io::Error>,
}

impl ModulePathVisitor<'_> {
    fn inspect_meta(&mut self, meta: &Meta) -> io::Result<()> {
        match meta {
            Meta::NameValue(value) if is_attribute_named(&value.path, "path") => {
                let Expr::Lit(expression) = &value.value else {
                    return Err(unsupported("a source module uses a computed path"));
                };
                let Lit::Str(path) = &expression.lit else {
                    return Err(unsupported("a source module uses a non-string path"));
                };
                self.inspect_path(&path.value(), true)
            }
            Meta::List(list) if is_attribute_named(&list.path, "cfg_attr") => {
                let nested = list
                    .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    .map_err(|_| unsupported("a source module uses an unsupported cfg_attr"))?;
                for attribute in nested {
                    self.inspect_meta(&attribute)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn inspect_path(&self, path: &str, uses_inline_module_directory: bool) -> io::Result<()> {
        if path.contains('\\') {
            return Err(unsupported(
                "a source module uses an unsupported escaped path",
            ));
        }

        let relative_path = Path::new(path);
        if relative_path.is_absolute() {
            return Err(unsupported("a source module uses an absolute path"));
        }
        if relative_path
            .extension()
            .is_none_or(|extension| extension != "rs")
        {
            return Err(unsupported("a source module uses a non-Rust path"));
        }

        let resolved = self.resolve_existing_module(relative_path, uses_inline_module_directory)?;

        self.validate_resolved_module(&resolved)
    }

    fn validate_resolved_module(&self, resolved: &Path) -> io::Result<()> {
        if !resolved.starts_with(self.package_root) {
            return Err(unsupported("a source module escapes its package"));
        }
        let relative = resolved
            .strip_prefix(self.package_root)
            .map_err(|_| unsupported("a source module escapes its package"))?;
        if relative.components().any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some(".git" | ".hg" | ".svn")
            )
        }) || resolved.starts_with(self.package_root.join("target"))
            || resolved.starts_with(self.cache_directory)
            || self
                .cache_directory
                .canonicalize()
                .is_ok_and(|directory| resolved.starts_with(directory))
        {
            return Err(unsupported("a source module lies in an excluded directory"));
        }
        if self.package_roots.iter().any(|root| {
            root != self.package_root
                && root.starts_with(self.package_root)
                && resolved.starts_with(root)
        }) {
            return Err(unsupported("a source module belongs to another package"));
        }

        Ok(())
    }

    fn inspect_implicit_module(&self, module: &syn::ItemMod) -> io::Result<()> {
        let name = module.ident.unraw().to_string();
        if !self
            .excluded_module_names
            .iter()
            .any(|directory| module_names_match(&name, directory))
        {
            return Ok(());
        }

        let Some(parent) = self.source_path.parent() else {
            return Err(unsupported("a source module has no parent directory"));
        };
        let mut directories = vec![parent.to_owned()];
        if let Some(stem) = self.source_path.file_stem() {
            let nested = parent.join(stem);
            if !directories.contains(&nested) {
                directories.push(nested);
            }
        }

        for mut directory in directories {
            for inline in &self.inline_modules {
                directory.push(inline);
            }

            let candidates = if module.content.is_some() {
                vec![directory.join(&name)]
            } else {
                vec![
                    directory.join(format!("{name}.rs")),
                    directory.join(&name).join("mod.rs"),
                ]
            };

            for candidate in candidates {
                match candidate.canonicalize() {
                    Ok(resolved) => self.validate_resolved_module(&resolved)?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => {
                        return Err(unsupported("a source module cannot be safely resolved"));
                    }
                }
            }
        }

        Ok(())
    }

    fn resolve_existing_module(
        &self,
        relative_path: &Path,
        uses_inline_module_directory: bool,
    ) -> io::Result<PathBuf> {
        let Some(parent) = self.source_path.parent() else {
            return Err(unsupported("a source module has no parent directory"));
        };

        let mut directories = Vec::new();
        if self.inline_modules.is_empty() || !uses_inline_module_directory {
            directories.push(parent.to_owned());
        } else {
            let mut direct = parent.to_owned();
            for module in &self.inline_modules {
                direct.push(module);
            }
            directories.push(direct);

            if let Some(stem) = self.source_path.file_stem() {
                let mut nested = parent.join(stem);
                for module in &self.inline_modules {
                    nested.push(module);
                }
                if !directories.contains(&nested) {
                    directories.push(nested);
                }
            }
        }

        let mut resolved = None;
        for directory in directories {
            let candidate = directory.join(relative_path);
            match candidate.canonicalize() {
                Ok(candidate) => {
                    if resolved
                        .as_ref()
                        .is_some_and(|previous: &PathBuf| *previous != candidate)
                    {
                        return Err(unsupported(
                            "an inline source module has ambiguous ownership",
                        ));
                    }
                    resolved = Some(candidate);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(unsupported("a source module cannot be safely resolved"));
                }
            }
        }

        resolved.ok_or_else(|| {
            unsupported(&format!(
                "a source module cannot be safely resolved: {} from {}",
                relative_path.display(),
                self.source_path.display()
            ))
        })
    }
}

fn is_attribute_named(path: &syn::Path, expected: &str) -> bool {
    path.get_ident()
        .is_some_and(|identifier| identifier.unraw() == expected)
}

impl<'ast> Visit<'ast> for ModulePathVisitor<'_> {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if self.error.is_some() {
            return;
        }
        for attribute in &module.attrs {
            if let Err(error) = self.inspect_meta(&attribute.meta) {
                self.error = Some(error);
                return;
            }
        }

        if self.inspect_implicit_modules
            && !module
                .attrs
                .iter()
                .any(|attribute| is_attribute_named(attribute.path(), "path"))
        {
            if let Err(error) = self.inspect_implicit_module(module) {
                self.error = Some(error);
                return;
            }
        }

        let inline = module.content.is_some();
        if inline {
            self.inline_modules.push(module.ident.unraw().to_string());
        }
        visit::visit_item_mod(self, module);
        if inline {
            self.inline_modules.pop();
        }
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if self.error.is_some() {
            return;
        }
        if item
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident.unraw() == "cfg_if")
            || item
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident.unraw() == "cfg_match")
        {
            self.error = Some(unsupported(
                "a conditional macro may resolve untracked source modules",
            ));
            return;
        }

        if item.path.is_ident("include") {
            match syn::parse2::<syn::LitStr>(item.tokens.clone()) {
                Ok(path) => {
                    if let Err(error) = self.inspect_path(&path.value(), false) {
                        self.error = Some(error);
                        return;
                    }
                }
                Err(_) => {
                    self.error = Some(unsupported("a source module uses a computed include path"));
                    return;
                }
            }
        }
        visit::visit_macro(self, item);
    }
}

fn collect_ancestor_configuration(
    directory: &Path,
    configuration: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    for ancestor in directory.ancestors() {
        for name in [
            "rustfmt.toml",
            ".rustfmt.toml",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            collect_configuration_file(&ancestor.join(name), configuration)?;
        }
        collect_configuration_file(&ancestor.join(".cargo/config"), configuration)?;
        collect_configuration_file(&ancestor.join(".cargo/config.toml"), configuration)?;
    }

    Ok(())
}

fn collect_global_configuration(configuration: &mut BTreeMap<String, String>) -> io::Result<()> {
    if let Some(directory) = dirs::home_dir() {
        collect_configuration_file(&directory.join(".rustfmt.toml"), configuration)?;
        collect_configuration_file(&directory.join("rustfmt.toml"), configuration)?;
    }

    if let Some(directory) = dirs::config_dir() {
        let directory = directory.join("rustfmt");
        collect_configuration_file(&directory.join(".rustfmt.toml"), configuration)?;
        collect_configuration_file(&directory.join("rustfmt.toml"), configuration)?;
    }

    Ok(())
}

fn collect_explicit_configuration(
    arguments: &[String],
    configuration: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let path = if argument == "--config-path" {
            arguments.next().map(PathBuf::from)
        } else {
            argument.strip_prefix("--config-path=").map(PathBuf::from)
        };

        let Some(path) = path else {
            continue;
        };

        if path.is_dir() {
            collect_configuration_file(&path.join("rustfmt.toml"), configuration)?;
            collect_configuration_file(&path.join(".rustfmt.toml"), configuration)?;
        } else {
            reject_observable_formatter_configuration(&path)?;
            collect_configuration_file(&path, configuration)?;
        }
    }

    Ok(())
}

fn collect_configuration_file(
    path: &Path,
    configuration: &mut BTreeMap<String, String>,
) -> io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }

    if matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("rustfmt.toml" | ".rustfmt.toml")
    ) {
        reject_observable_formatter_configuration(path)?;
    }

    let path = path.canonicalize()?;
    let key = path_string(&path)?;
    if !configuration.contains_key(&key) {
        configuration.insert(key, hash_file(&path)?);
    }
    Ok(())
}

fn reject_observable_formatter_configuration(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }

    let configuration = fs::read(path)?;
    if configuration.contains(&b'\\')
        || [
            b"emit_mode".as_slice(),
            b"file_lines".as_slice(),
            b"verbose".as_slice(),
            b"print_misformatted_file_names".as_slice(),
            b"make_backup".as_slice(),
        ]
        .iter()
        .any(|setting| {
            configuration
                .windows(setting.len())
                .any(|candidate| candidate == *setting)
        })
    {
        return Err(unsupported(
            "a rustfmt configuration may select observable formatting behavior",
        ));
    }

    Ok(())
}

fn is_configuration_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            "rustfmt.toml"
                | ".rustfmt.toml"
                | "Cargo.toml"
                | "Cargo.lock"
                | "rust-toolchain"
                | "rust-toolchain.toml"
        )
    )
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 16 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }

    Ok(format!("{:x}", digest.finalize()))
}

fn trusted_formatter() -> io::Result<PathBuf> {
    if env::var_os("RUSTFMT_WRAPPER").is_some() {
        return Err(unsupported(
            "a custom rustfmt wrapper cannot be cached safely",
        ));
    }

    let path = PathBuf::from(rustfmt_command().get_program()).canonicalize()?;
    let expected_formatter = if cfg!(windows) {
        "rustfmt.exe"
    } else {
        "rustfmt"
    };
    let expected_compiler = if cfg!(windows) { "rustc.exe" } else { "rustc" };
    let current_toolchain = env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_owned))
        .is_some_and(|directory| {
            directory
                .join(expected_formatter)
                .canonicalize()
                .is_ok_and(|formatter| formatter == path)
                && directory.join(expected_compiler).is_file()
        });
    let rustup_toolchain = env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".rustup")))
        .and_then(|root| fs::read_dir(root.join("toolchains")).ok())
        .is_some_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                let directory = entry.path().join("bin");
                directory
                    .join(expected_formatter)
                    .canonicalize()
                    .is_ok_and(|formatter| formatter == path)
                    && directory.join(expected_compiler).is_file()
            })
        });
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_formatter)
        || (!current_toolchain && !rustup_toolchain)
    {
        return Err(unsupported(
            "a custom rustfmt executable cannot be cached safely",
        ));
    }

    let mut file = File::open(&path)?;
    let mut prefix = [0; 4];
    file.read_exact(&mut prefix)?;
    let native_executable = prefix == *b"\x7fELF"
        || prefix[..2] == *b"MZ"
        || matches!(
            prefix,
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
                | [0xca, 0xfe, 0xba, 0xbf]
        );
    if !native_executable {
        return Err(unsupported(
            "a scripted rustfmt wrapper cannot be cached safely",
        ));
    }

    Ok(path)
}

fn hash_value(digest: &mut Sha256, value: &str) {
    digest.update(value.len().to_le_bytes());
    digest.update(value.as_bytes());
}

fn hash_environment(digest: &mut Sha256, variable: &str) -> io::Result<()> {
    hash_value(digest, variable);
    let Some(value) = env::var_os(variable) else {
        hash_value(digest, "<unset>");
        return Ok(());
    };
    let Some(value) = value.to_str() else {
        return Err(unsupported(
            "a formatter environment value is not valid UTF-8",
        ));
    };
    hash_value(digest, value);
    Ok(())
}

fn path_string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| unsupported("a package path is not valid UTF-8"))
}

fn cache_directory(manifest_path: Option<&Path>) -> io::Result<PathBuf> {
    if let Some(path) = env::var_os(CACHE_ENVIRONMENT) {
        return Ok(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("CARGO_TARGET_DIR") {
        return Ok(PathBuf::from(path).join("ohm-rustfmt"));
    }

    let directory = if let Some(path) = manifest_path {
        path.parent()
            .map(Path::to_owned)
            .unwrap_or(env::current_dir()?)
    } else {
        env::current_dir()?
    };

    Ok(directory.join("target").join("ohm-rustfmt"))
}

fn write_cache(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(unsupported("the cache path has no parent directory"));
    };

    fs::create_dir_all(parent)?;
    let temporary_path = path.with_extension(format!("{}.tmp", std::process::id()));
    let file = File::create(&temporary_path)?;
    serde_json::to_writer(file, snapshot).map_err(io::Error::other)?;
    fs::rename(temporary_path, path)
}

fn has_observable_emit_mode(arguments: &[String]) -> bool {
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if matches!(
            argument.as_str(),
            "--print-config"
                | "--config-help"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
                | "--verbose"
                | "-v"
                | "--file-lines"
        ) || argument.starts_with("--print-config=")
            || argument.starts_with("--help=")
            || argument.starts_with("--file-lines=")
            || argument == "--files-with-diff"
            || (argument.starts_with('-')
                && !argument.starts_with("--")
                && argument
                    .chars()
                    .skip(1)
                    .any(|flag| matches!(flag, 'v' | 'V' | 'h' | 'l')))
        {
            return true;
        } else if argument == "--emit" {
            if arguments.next().is_none_or(|mode| mode != "files") {
                return true;
            }
        } else if argument
            .strip_prefix("--emit=")
            .is_some_and(|mode| mode != "files")
        {
            return true;
        } else if argument == "--config" || argument.starts_with("--config=") {
            return true;
        }
    }

    false
}

fn trace(message: &str) {
    if matches!(env::var(TRACE_ENVIRONMENT).as_deref(), Ok("1" | "true")) {
        eprintln!("ohm incremental fmt: {message}");
    }
}

fn unsupported(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(files: &[(&str, &[(&str, &str)])]) -> Snapshot {
        Snapshot {
            version: CACHE_VERSION,
            invocation: String::from("formatter-and-arguments"),
            configuration: BTreeMap::from([(
                String::from("/workspace/rustfmt.toml"),
                String::from("original-config"),
            )]),
            packages: files
                .iter()
                .map(|(package, sources)| {
                    (
                        String::from(*package),
                        sources
                            .iter()
                            .map(|(path, hash)| (String::from(*path), String::from(*hash)))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    fn plan(previous: Option<Snapshot>, current: Snapshot) -> Plan {
        Plan {
            cache_path: PathBuf::from("unused-cache.json"),
            previous,
            current,
        }
    }

    #[test]
    fn unchanged_sources_hit_the_successful_format_cache() {
        let current = snapshot(&[("/workspace/first", &[("src/lib.rs", "formatted")])]);
        let plan = plan(Some(current.clone()), current);

        assert!(plan.changed_packages().is_empty());
    }

    #[test]
    fn changed_source_only_invalidates_its_own_package() {
        let previous = snapshot(&[
            ("/workspace/first", &[("src/lib.rs", "before")]),
            ("/workspace/second", &[("src/lib.rs", "unchanged")]),
        ]);
        let current = snapshot(&[
            ("/workspace/first", &[("src/lib.rs", "after")]),
            ("/workspace/second", &[("src/lib.rs", "unchanged")]),
        ]);
        let plan = plan(Some(previous), current);

        assert_eq!(
            plan.changed_packages(),
            BTreeSet::from(["/workspace/first"])
        );
    }

    #[test]
    fn added_and_deleted_modules_invalidate_their_package() {
        let previous = snapshot(&[("/workspace/first", &[("src/lib.rs", "unchanged")])]);
        let added = snapshot(&[(
            "/workspace/first",
            &[("src/lib.rs", "unchanged"), ("src/new.rs", "new")],
        )]);
        let addition = plan(Some(previous.clone()), added.clone());
        let deletion = plan(Some(added), previous);

        assert_eq!(
            addition.changed_packages(),
            BTreeSet::from(["/workspace/first"])
        );
        assert_eq!(
            deletion.changed_packages(),
            BTreeSet::from(["/workspace/first"])
        );
    }

    #[test]
    fn configuration_changes_invalidate_every_package() {
        let previous = snapshot(&[
            ("/workspace/first", &[("src/lib.rs", "unchanged")]),
            ("/workspace/second", &[("src/lib.rs", "unchanged")]),
        ]);
        let mut current = previous.clone();
        current.configuration.insert(
            String::from("/workspace/rustfmt.toml"),
            String::from("changed-config"),
        );
        let plan = plan(Some(previous), current);

        assert_eq!(
            plan.changed_packages(),
            BTreeSet::from(["/workspace/first", "/workspace/second"])
        );
    }

    #[test]
    fn concurrently_changed_unformatted_packages_are_not_cached() {
        let current = snapshot(&[
            ("/workspace/first", &[("src/lib.rs", "before-formatting")]),
            ("/workspace/second", &[("src/lib.rs", "unchanged")]),
        ]);
        let updated = snapshot(&[
            ("/workspace/first", &[("src/lib.rs", "formatted")]),
            (
                "/workspace/second",
                &[("src/lib.rs", "edited-concurrently")],
            ),
        ]);
        let plan = plan(None, current);

        assert!(!plan.can_record(&updated, &BTreeSet::from(["/workspace/first"]), false));
    }

    #[test]
    fn concurrently_changed_selected_packages_are_not_cached_after_checking() {
        let current = snapshot(&[("/workspace/first", &[("src/lib.rs", "before-checking")])]);
        let updated = snapshot(&[("/workspace/first", &[("src/lib.rs", "edited-concurrently")])]);
        let plan = plan(None, current);

        assert!(!plan.can_record(&updated, &BTreeSet::from(["/workspace/first"]), true));
    }

    #[test]
    fn stdout_and_json_emit_modes_bypass_the_cache() {
        assert!(has_observable_emit_mode(&[
            String::from("--emit"),
            String::from("stdout"),
        ]));
        assert!(has_observable_emit_mode(&[String::from("--emit=json")]));
        assert!(has_observable_emit_mode(&[String::from("-v")]));
        assert!(has_observable_emit_mode(&[String::from("-lv")]));
        assert!(has_observable_emit_mode(&[String::from("-lV")]));
        assert!(has_observable_emit_mode(&[String::from("-qh")]));
        assert!(has_observable_emit_mode(&[String::from("-l")]));
        assert!(has_observable_emit_mode(&[String::from(
            "--files-with-diff"
        )]));
        assert!(has_observable_emit_mode(&[String::from("--config-help")]));
        assert!(has_observable_emit_mode(&[String::from("--file-lines=[]")]));
        assert!(has_observable_emit_mode(&[
            String::from("--config"),
            String::from("emit_mode=Stdout"),
        ]));
        assert!(has_observable_emit_mode(&[String::from(
            "--config=emit_mode=Stdout"
        )]));
        assert!(has_observable_emit_mode(&[
            String::from("--config"),
            String::from("max_width=100,emit_mode=Stdout"),
        ]));
        assert!(has_observable_emit_mode(&[
            String::from("--config"),
            String::from("verbose=Verbose"),
        ]));
        assert!(has_observable_emit_mode(&[String::from(
            "--config=file_lines=[]"
        )]));
        assert!(!has_observable_emit_mode(&[
            String::from("--emit"),
            String::from("files"),
        ]));
        assert!(has_observable_emit_mode(&[
            String::from("--config"),
            String::from("max_width=100"),
        ]));
        assert!(!has_observable_emit_mode(&[String::from("-q")]));
    }

    #[test]
    fn verification_removes_explicit_file_emission_before_checking() {
        assert_eq!(
            verification_arguments(&[String::from("--emit"), String::from("files")]),
            vec![String::from("--check")]
        );
        assert_eq!(
            verification_arguments(&[String::from("--emit=files")]),
            vec![String::from("--check")]
        );
        assert_eq!(
            verification_arguments(&[String::from("--check")]),
            vec![String::from("--check")]
        );
    }

    #[test]
    fn escaping_module_paths_require_complete_formatting() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        fs::create_dir_all(package.join("src"))?;
        fs::create_dir_all(package.join("tests"))?;
        fs::write(package.join("tests/local.rs"), "pub fn local() {}")?;
        let package = package.canonicalize()?;
        let packages = BTreeSet::from([package.clone()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        assert!(
            validate_module_paths(
                &source,
                br##"#[path = "../../external.rs"] mod external;"##,
                &package,
                &packages,
                &cache,
            )
            .is_err()
        );
        assert!(
            validate_module_paths(
                &source,
                br##"#[path = "../tests/local.rs"] mod local;"##,
                &package,
                &packages,
                &cache,
            )
            .is_ok()
        );
        assert!(
            validate_module_paths(
                &source,
                br##"#[path = "..\x2f..\x2fexternal.rs"] mod external;"##,
                &package,
                &packages,
                &cache,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn comments_and_configuration_attributes_cannot_hide_external_modules() {
        let package = Path::new("/workspace/package");
        let packages = BTreeSet::from([package.to_path_buf()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        for contents in [
            br##"#[path /* still a valid attribute */ = "../../external.rs"] mod external;"##
                .as_slice(),
            br##"#[r#path = "../../external.rs"] mod external;"##.as_slice(),
            br##"#[cfg_attr(test, path = "../../external.rs")] mod external;"##.as_slice(),
            br##"#[r#cfg_attr(test, r#path = "../../external.rs")] mod external;"##.as_slice(),
            br##"include!("../../external.rs");"##.as_slice(),
        ] {
            assert!(
                validate_module_paths(&source, contents, package, &packages, &cache).is_err(),
                "the external module must not be cached: {}",
                String::from_utf8_lossy(contents)
            );
        }
    }

    #[test]
    fn conditional_macros_cannot_hide_external_modules() {
        let package = Path::new("/workspace/package");
        let packages = BTreeSet::from([package.to_path_buf()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        for contents in [
            br##"cfg_if! { if #[cfg(unix)] { #[path = "../../shared.rs"] mod shared; } }"##
                .as_slice(),
            br##"std::cfg_match! { unix => { #[path = "../../shared.rs"] mod shared; } }"##
                .as_slice(),
        ] {
            assert!(
                validate_module_paths(&source, contents, package, &packages, &cache).is_err(),
                "the conditional macro must not hide an external module: {}",
                String::from_utf8_lossy(contents)
            );
        }
    }

    #[test]
    fn rust_pattern_whitespace_cannot_hide_module_attributes() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        fs::create_dir_all(package.join("src"))?;
        fs::write(directory.path().join("external.rs"), "pub fn external() {}")?;
        let package = package.canonicalize()?;
        let packages = BTreeSet::from([package.clone()]);
        let source_path = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        for separator in [
            '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{0085}', '\u{200e}',
            '\u{200f}', '\u{2028}', '\u{2029}',
        ] {
            let source = format!("#{separator}[path = \"../../external.rs\"] mod external;");
            assert!(
                has_module_resolution_syntax(&source),
                "Rust whitespace U+{:04X} must not hide module attributes",
                separator as u32
            );
            assert!(
                validate_module_paths(
                    &source_path,
                    source.as_bytes(),
                    &package,
                    &packages,
                    &cache,
                )
                .is_err(),
                "Rust whitespace U+{:04X} must not bypass ownership validation",
                separator as u32
            );
        }

        Ok(())
    }

    #[test]
    fn canonical_paths_reject_case_insensitive_aliases() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        fs::create_dir_all(package.join("src"))?;
        fs::create_dir_all(package.join("target"))?;
        fs::write(package.join("target/generated.rs"), "pub fn generated() {}")?;
        let package = package.canonicalize()?;

        if !package.join("TARGET/generated.rs").is_file() {
            return Ok(());
        }

        let packages = BTreeSet::from([package.clone()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");
        assert!(
            validate_module_paths(
                &source,
                br##"#[path = "../TARGET/generated.rs"] mod generated;"##,
                &package,
                &packages,
                &cache,
            )
            .is_err()
        );

        Ok(())
    }

    #[test]
    fn inline_modules_resolve_relative_to_their_module_directory() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        let member = package.join("src/member");
        fs::create_dir_all(package.join("src/outer"))?;
        fs::create_dir_all(&member)?;
        fs::write(member.join("shared.rs"), "pub fn shared() {}")?;
        let package = package.canonicalize()?;
        let member = member.canonicalize()?;

        let packages = BTreeSet::from([package.clone(), member]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");
        assert!(
            validate_module_paths(
                &source,
                br##"mod outer { #[path = "../member/shared.rs"] mod shared; }"##,
                &package,
                &packages,
                &cache,
            )
            .is_err()
        );

        Ok(())
    }

    #[test]
    fn includes_in_inline_modules_stay_relative_to_the_containing_file() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        fs::create_dir_all(package.join("src"))?;
        fs::write(package.join("src/included.rs"), "pub fn included() {}")?;
        let package = package.canonicalize()?;
        let packages = BTreeSet::from([package.clone()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        assert!(
            validate_module_paths(
                &source,
                br##"mod tests { include!("included.rs"); }"##,
                &package,
                &packages,
                &cache,
            )
            .is_ok()
        );

        Ok(())
    }

    #[test]
    fn excluded_and_nested_member_modules_require_complete_formatting() {
        let package = Path::new("/workspace/package");
        let nested = package.join("nested");
        let packages = BTreeSet::from([package.to_path_buf(), nested]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        for contents in [
            br##"#[path = "../target/generated.rs"] mod generated;"##.as_slice(),
            br##"#[path = "../nested/shared.rs"] mod nested;"##.as_slice(),
            br##"#[path = "../.hg/generated.rs"] mod hidden;"##.as_slice(),
        ] {
            assert!(
                validate_module_paths(&source, contents, package, &packages, &cache).is_err(),
                "the cross-owned module must not be cached: {}",
                String::from_utf8_lossy(contents)
            );
        }
    }

    #[test]
    fn implicit_modules_cannot_enter_excluded_or_nested_directories() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        let target = package.join("target");
        let cache = package.join("generated");
        let member = package.join("src/member");
        fs::create_dir_all(&target)?;
        fs::create_dir_all(&cache)?;
        fs::create_dir_all(&member)?;
        fs::write(target.join("mod.rs"), "pub fn target() {}")?;
        fs::write(cache.join("mod.rs"), "pub fn generated() {}")?;
        fs::write(member.join("mod.rs"), "pub fn member() {}")?;

        let package = package.canonicalize()?;
        let member = member.canonicalize()?;
        let packages = BTreeSet::from([package.clone(), member]);
        let cache = cache.canonicalize()?;

        for (source, contents) in [
            (package.join("lib.rs"), "mod target;"),
            (package.join("lib.rs"), "mod r#target;"),
            (package.join("lib.rs"), "mod generated;"),
            (package.join("src/lib.rs"), "mod member;"),
            (package.join("lib.rs"), "mod target { mod generated; }"),
            (
                package.join("lib.rs"),
                "cfg_if! { if #[cfg(unix)] { mod target; } }",
            ),
        ] {
            assert!(
                validate_module_paths(&source, contents.as_bytes(), &package, &packages, &cache,)
                    .is_err(),
                "the implicit module must not be cached: {contents}"
            );
        }

        if package.join("TARGET/mod.rs").is_file() {
            assert!(
                validate_module_paths(
                    &package.join("lib.rs"),
                    b"mod TARGET;",
                    &package,
                    &packages,
                    &cache,
                )
                .is_err()
            );
        }

        Ok(())
    }

    #[test]
    fn tracked_target_named_modules_keep_the_incremental_fast_path() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("package");
        fs::create_dir_all(package.join("target"))?;
        fs::create_dir_all(package.join("src/target"))?;
        fs::write(package.join("src/target/mod.rs"), "pub fn target() {}")?;
        let package = package.canonicalize()?;
        let packages = BTreeSet::from([package.clone()]);

        assert!(
            validate_module_paths(
                &package.join("src/lib.rs"),
                b"mod target;",
                &package,
                &packages,
                &package.join("format-cache"),
            )
            .is_ok()
        );

        Ok(())
    }

    #[test]
    fn attribute_like_text_in_strings_does_not_escape_package_ownership() {
        let package = Path::new("/workspace/package");
        let packages = BTreeSet::from([package.to_path_buf()]);
        let source = package.join("src/lib.rs");
        let cache = package.join("target/format-cache");

        assert!(
            validate_module_paths(
                &source,
                br####"const EXAMPLE: &str = r##"#[path = "../../external.rs"]"##;"####,
                package,
                &packages,
                &cache,
            )
            .is_ok()
        );
    }

    #[test]
    fn generic_path_words_do_not_trigger_full_syntax_parsing() {
        assert!(!has_module_resolution_syntax(
            "#[derive(Debug)]\nfn accepts(path: &str) { println!(\"{path}\"); }"
        ));
        assert!(!has_module_resolution_syntax(
            "#[cfg_attr(feature = \"extra\", derive(Debug))]\nfn accepts(path: &str) {}"
        ));
        assert!(has_module_resolution_syntax(
            "# /* outer /* nested */ trivia */ [ /* attr */ path = \"local.rs\"] mod local;"
        ));
        assert!(has_module_resolution_syntax(
            "#[cfg_attr(feature = \"extra\", path = \"local.rs\")] mod local;"
        ));
        assert!(has_module_resolution_syntax(
            "include /* trivia */ !(\"local.rs\");"
        ));
        let excluded = BTreeSet::from([String::from("target"), String::from("member")]);
        assert!(has_implicit_excluded_module("mod target;", &excluded));
        assert!(has_implicit_excluded_module(
            "mod /* trivia */ r#target;",
            &excluded
        ));
        assert!(has_implicit_excluded_module("mod TARGET;", &excluded));
        assert!(has_implicit_excluded_module(
            "cfg_if! { if #[cfg(unix)] { mod member; } }",
            &excluded
        ));
        assert!(!has_implicit_excluded_module("mod tracked;", &excluded));
        assert!(!has_implicit_excluded_module(
            "fn module_target() {}",
            &excluded
        ));
    }
}
