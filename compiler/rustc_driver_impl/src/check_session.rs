//! The private process protocol used by an explicitly owned Cargo check session.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use rustc_data_structures::profiling::get_resident_set_size;
use rustc_interface::interface::Config;
use rustc_interface::resident::ResidentFrontend;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::{ExternLocation, Externs};
use rustc_session::search_paths::PathKind;
use rustc_session::utils::CanonicalizedPath;
use rustc_span::source_map::{FileLoader, RealFileLoader};
use rustc_span::{SourceFileHash, SourceFileHashAlgorithm};
use serde_json::{Value, json};

const MAX_REQUEST_BYTES: u64 = 4096;
const MAX_REVISIONS: u64 = 256;
const MAX_RSS: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
enum Input {
    Unreadable,
    Exists(bool),
    Contents(SourceFileHash),
}

#[derive(Default)]
struct Inputs {
    files: BTreeMap<PathBuf, Input>,
    identities: BTreeMap<PathBuf, Option<PathBuf>>,
    changed_while_reading: bool,
}

impl Inputs {
    fn record(&mut self, path: &Path, input: Input) {
        let identity = fs::canonicalize(path).ok();
        if self
            .identities
            .insert(path.to_owned(), identity.clone())
            .is_some_and(|previous| previous != identity)
        {
            self.changed_while_reading = true;
        }
        match self.files.get(path) {
            Some(Input::Exists(false)) if matches!(input, Input::Contents(_)) => {
                self.changed_while_reading = true;
            }
            Some(Input::Contents(_)) if matches!(input, Input::Exists(false)) => {
                self.changed_while_reading = true;
            }
            Some(Input::Contents(previous)) if matches!(&input, Input::Contents(next) if next != previous) =>
            {
                self.changed_while_reading = true;
            }
            Some(Input::Exists(previous)) if matches!(&input, Input::Exists(next) if next != previous) =>
            {
                self.changed_while_reading = true;
            }
            Some(Input::Contents(_)) if matches!(input, Input::Exists(true)) => return,
            _ => {}
        }
        self.files.insert(path.to_owned(), input);
    }

    fn unchanged(&self, edited: Option<&Path>) -> bool {
        !self.changed_while_reading
            && self.files.iter().all(|(path, input)| {
                let identity = fs::canonicalize(path).ok();
                if self.identities.get(path) != Some(&identity) {
                    return false;
                }
                if edited.is_some_and(|edited| identity.as_deref() == Some(edited)) {
                    return true;
                }
                match input {
                    Input::Unreadable => false,
                    Input::Exists(expected) => path.exists() == *expected,
                    Input::Contents(expected) => {
                        hash_file(path).is_ok_and(|hash| hash == *expected)
                    }
                }
            })
    }
}

struct RecordingLoader(Arc<Mutex<Inputs>>);

impl FileLoader for RecordingLoader {
    fn file_exists(&self, path: &Path) -> bool {
        let exists = RealFileLoader.file_exists(path);
        self.0.lock().unwrap().record(path, Input::Exists(exists));
        exists
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        let source = RealFileLoader.read_file(path).map_err(|error| {
            self.0.lock().unwrap().record(
                path,
                if error.kind() == io::ErrorKind::NotFound {
                    Input::Exists(false)
                } else {
                    Input::Unreadable
                },
            );
            error
        })?;
        self.0.lock().unwrap().record(path, Input::Contents(hash_bytes(source.as_bytes())));
        Ok(source)
    }

    fn read_binary_file(&self, path: &Path) -> io::Result<Arc<[u8]>> {
        let bytes = RealFileLoader.read_binary_file(path).map_err(|error| {
            self.0.lock().unwrap().record(
                path,
                if error.kind() == io::ErrorKind::NotFound {
                    Input::Exists(false)
                } else {
                    Input::Unreadable
                },
            );
            error
        })?;
        self.0.lock().unwrap().record(path, Input::Contents(hash_bytes(&bytes)));
        Ok(bytes)
    }

    fn current_directory(&self) -> io::Result<PathBuf> {
        RealFileLoader.current_directory()
    }
}

fn hash_bytes(bytes: &[u8]) -> SourceFileHash {
    SourceFileHash::new_in_memory(SourceFileHashAlgorithm::Blake3, bytes)
}

fn hash_file(path: &Path) -> io::Result<SourceFileHash> {
    SourceFileHash::new(SourceFileHashAlgorithm::Blake3, fs::File::open(path)?)
}

fn artifacts(
    directories: &[PathBuf],
    exact: &[PathBuf],
) -> io::Result<BTreeMap<PathBuf, SourceFileHash>> {
    let mut paths = BTreeSet::from_iter(exact.iter().cloned());
    for directory in directories {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rmeta" | "rlib" | "so" | "dylib" | "dll")
            ) && path.is_file()
            {
                paths.insert(path);
            }
        }
    }
    paths.into_iter().map(|path| hash_file(&path).map(|hash| (path, hash))).collect()
}

pub(crate) struct CheckSession {
    source: PathBuf,
    inputs: Arc<Mutex<Inputs>>,
    directories: Vec<PathBuf>,
    exact: Vec<PathBuf>,
    artifacts: BTreeMap<PathBuf, SourceFileHash>,
    requests: Mutex<mpsc::Receiver<String>>,
    _snapshot: tempfile::TempDir,
}

impl CheckSession {
    pub(crate) fn configure(config: &mut Config) -> io::Result<Self> {
        if config.file_loader.is_some() {
            return Err(io::Error::other("check sessions require the filesystem loader"));
        }
        if matches!(config.opts.target_triple, rustc_target::spec::TargetTuple::TargetJson { .. }) {
            return Err(io::Error::other("custom target specifications require a fresh check"));
        }
        let (sender, requests) = mpsc::channel();
        std::thread::spawn(move || {
            let mut input = io::stdin().lock();
            loop {
                let mut request = String::new();
                match std::io::Read::take(&mut input, MAX_REQUEST_BYTES).read_line(&mut request) {
                    Ok(0) | Err(_) => std::process::exit(0),
                    Ok(_) if request.ends_with('\n') => {
                        if sender.send(request).is_err() {
                            return;
                        }
                    }
                    _ => std::process::exit(1),
                }
            }
        });
        let source =
            fs::canonicalize(config.opts.unstable_opts.ohm_resident_frontend.as_ref().unwrap())?;
        let inputs = Arc::new(Mutex::new(Inputs::default()));
        config.file_loader = Some(Box::new(RecordingLoader(Arc::clone(&inputs))));
        let mut directories: Vec<_> = config
            .opts
            .search_paths
            .iter()
            .filter(|path| path.kind != PathKind::Native && path.kind != PathKind::Framework)
            .map(|path| path.dir.to_path_buf())
            .collect();
        let dependency_directories = directories.clone();
        directories.push(config.opts.sysroot.path().join("lib"));
        let sysroot_lib = config.opts.sysroot.path().join("lib/rustlib");
        if sysroot_lib.is_dir() {
            for entry in fs::read_dir(sysroot_lib)? {
                let path = entry?.path().join("lib");
                if path.is_dir() {
                    directories.push(path);
                }
            }
        }
        directories.sort();
        directories.dedup();
        let exact: Vec<_> = config
            .opts
            .externs
            .iter()
            .flat_map(|(_, entry)| entry.files().into_iter().flatten())
            .map(|path| path.original().to_owned())
            .collect();
        let expected_artifacts = artifacts(&directories, &exact)?;
        let output = config
            .output_dir
            .as_ref()
            .ok_or_else(|| io::Error::other("check sessions require a private output directory"))?;
        fs::create_dir_all(output)?;
        let snapshot = tempfile::Builder::new().prefix("inputs-").tempdir_in(output)?;
        let mut copies = BTreeMap::new();
        let mut copied_directories = BTreeMap::new();
        for (index, directory) in dependency_directories.iter().enumerate() {
            let destination = snapshot.path().join(format!("search-{index}"));
            fs::create_dir_all(&destination)?;
            for (path, expected) in &expected_artifacts {
                if path.parent() == Some(directory.as_path()) {
                    let copy = destination.join(path.file_name().unwrap());
                    fs::copy(path, &copy)?;
                    if hash_file(&copy)? != *expected {
                        return Err(io::Error::other(
                            "dependency changed while making its private snapshot",
                        ));
                    }
                    copies.insert(path.clone(), copy);
                }
            }
            copied_directories.insert(directory.clone(), destination);
        }
        for (index, path) in exact.iter().enumerate() {
            if copies.contains_key(path) {
                continue;
            }
            let directory = snapshot.path().join(format!("extern-{index}"));
            fs::create_dir_all(&directory)?;
            let copy = directory
                .join(path.file_name().ok_or_else(|| io::Error::other("invalid extern path"))?);
            fs::copy(path, &copy)?;
            if hash_file(&copy)? != expected_artifacts[path] {
                return Err(io::Error::other("extern changed while making its private snapshot"));
            }
            copies.insert(path.clone(), copy);
        }
        for search in &mut config.opts.search_paths {
            if let Some(copy) = copied_directories.get(search.dir.as_ref()) {
                search.dir = Arc::from(copy.as_path());
            }
        }
        config.opts.externs = Externs::new(
            config
                .opts
                .externs
                .iter()
                .map(|(name, entry)| {
                    let mut entry = entry.clone();
                    if let ExternLocation::ExactPaths(paths) = &mut entry.location {
                        *paths = paths
                            .iter()
                            .map(|path| CanonicalizedPath::new(copies[path.original()].clone()))
                            .collect();
                    }
                    (name.clone(), entry)
                })
                .collect(),
        );
        if artifacts(&directories, &exact)? != expected_artifacts {
            return Err(io::Error::other("dependencies changed during snapshot creation"));
        }
        Ok(Self {
            source,
            inputs,
            directories,
            exact,
            artifacts: expected_artifacts,
            requests: Mutex::new(requests),
            _snapshot: snapshot,
        })
    }

    fn unchanged(&self, edited: bool) -> bool {
        self.inputs.lock().unwrap().unchanged(edited.then_some(self.source.as_path()))
            && artifacts(&self.directories, &self.exact)
                .is_ok_and(|current| current == self.artifacts)
    }

    pub(crate) fn run<'ra, 'tcx>(
        &self,
        tcx: TyCtxt<'tcx>,
        frontend: Option<&mut ResidentFrontend<'ra, 'tcx>>,
    ) {
        let Some(frontend) = frontend else {
            let _ = event("unsupported", 0, false, false, "unsupported source boundary", &[]);
            return;
        };
        if tcx
            .sess
            .file_depinfo
            .borrow()
            .iter()
            .any(|path| fs::canonicalize(path.as_str()).is_ok_and(|path| path == self.source))
        {
            unsupported("the edited source is also a macro input");
            return;
        }
        // Arbitrary procedural-macro side effects are not a tracked session input.
        if !tcx.sess.opts.unstable_opts.ohm_check_session_trust_proc_macros
            && tcx.crates(()).iter().any(|&krate| {
                let source = tcx.used_crate_source(krate);
                source.dylib.is_some() && source.rlib.is_none() && source.rmeta.is_none()
            })
        {
            let _ = event(
                "unsupported",
                0,
                false,
                false,
                "procedural macros require fresh expansion",
                &[],
            );
            return;
        }
        if tcx.sess.opts.unstable_opts.ohm_check_session_trust_proc_macros {
            for path in tcx.sess.file_depinfo.borrow().iter() {
                let path = PathBuf::from(path.as_str());
                let Ok(hash) = hash_file(&path) else {
                    unsupported("cannot read a declared procedural-macro input");
                    return;
                };
                let mut inputs = self.inputs.lock().unwrap();
                if !inputs.files.contains_key(&path) {
                    inputs.record(&path, Input::Contents(hash));
                }
            }
        }
        if tcx.sess.file_depinfo.borrow().iter().any(|path| {
            let Ok(path) = fs::canonicalize(path.as_str()) else { return true };
            !self.inputs.lock().unwrap().files.iter().any(|(read, input)| {
                matches!(input, Input::Contents(_))
                    && fs::canonicalize(read).is_ok_and(|read| read == path)
            })
        }) {
            let _ = event(
                "unsupported",
                0,
                false,
                false,
                "untracked file input requires a fresh check",
                &[],
            );
            return;
        }
        if let Err(reason) = frontend.prepare(tcx) {
            let _ = event("unsupported", 0, false, false, reason, &[]);
            return;
        }
        if !self.unchanged(false) {
            let _ = event(
                "unsupported",
                0,
                false,
                false,
                "inputs changed during initial analysis",
                &[],
            );
            return;
        }
        let files: Vec<_> = self.inputs.lock().unwrap().files.keys().cloned().collect();
        let retained = get_resident_set_size().is_some_and(|rss| (rss as u64) < MAX_RSS);
        if event("ready", 0, tcx.dcx().has_errors().is_some(), retained, "", &files).is_err()
            || !retained
        {
            return;
        }
        let mut revision = 0;
        while let Ok(request) = self.requests.lock().unwrap().recv() {
            let Ok(request) = serde_json::from_str::<Value>(&request) else { break };
            if request["command"] == "stop" {
                let _ = event("stopped", revision, false, false, "", &[]);
                break;
            }
            if request["command"] != "check" || request["revision"].as_u64() != Some(revision + 1) {
                break;
            }
            revision += 1;
            if !self.unchanged(true) {
                let _ = event("unsupported", revision, false, false, "session inputs changed", &[]);
                break;
            }
            let Ok(source) = fs::read_to_string(&self.source) else { break };
            let expected = hash_bytes(source.as_bytes());
            // SAFETY: This loop runs after analysis and its scoped frontend jobs have joined.
            // The input reader only sends strings; no other task can request compiler queries.
            // Each replacement finishes analysis before returning, and the loop retains no
            // semantic query results. Mutation boundaries also drain the query-cycle handler.
            if let Err(reason) = unsafe { frontend.replace_source(tcx, source) } {
                let _ = event("unsupported", revision, false, false, reason, &[]);
                break;
            }
            if !self.unchanged(true) || !hash_file(&self.source).is_ok_and(|hash| hash == expected)
            {
                let _ = event(
                    "unsupported",
                    revision,
                    false,
                    false,
                    "inputs changed during checking",
                    &[],
                );
                break;
            }
            let retained = revision < MAX_REVISIONS
                && get_resident_set_size().is_some_and(|rss| (rss as u64) < MAX_RSS);
            if event("checked", revision, tcx.dcx().has_errors().is_some(), retained, "", &files)
                .is_err()
                || !retained
            {
                break;
            }
        }
    }
}

fn event(
    kind: &str,
    revision: u64,
    errors: bool,
    retained: bool,
    reason: &str,
    files: &[PathBuf],
) -> io::Result<()> {
    // The boundary shares the diagnostic pipe so it cannot overtake earlier diagnostics.
    let mut stderr = io::stderr().lock();
    serde_json::to_writer(
        &mut stderr,
        &json!({
            "$message_type": "ohm-check-session", "protocol": 1, "kind": kind,
            "revision": revision, "errors": errors, "retained": retained,
            "reason": reason, "files": files,
        }),
    )?;
    stderr.write_all(b"\n")?;
    stderr.flush()
}

pub(crate) fn unsupported(reason: &str) {
    let _ = event("unsupported", 0, false, false, reason, &[]);
}

#[cfg(test)]
mod tests;
