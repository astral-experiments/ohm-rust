use std::mem::size_of;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fs, io};

use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::owned_slice::slice_owned;
use rustc_data_structures::temp_dir::MaybeTempDir;
use rustc_fs_util::TempDirBuilder;
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_session::config::{CrateType, OutFileName, OutputType};
use rustc_session::output::filename_for_metadata;

use crate::diagnostics::{
    BinaryOutputToTty, FailedCopyToStdout, FailedCreateEncodedMetadata, FailedCreateFile,
    FailedCreateTempdir, FailedWriteError,
};
use crate::rmeta::MetadataBlob;
use crate::{EncodedMetadata, METADATA_HEADER, encode_metadata};

// FIXME(eddyb) maybe include the crate name in this?
pub const METADATA_FILENAME: &str = "lib.rmeta";

pub struct EarlyMetadata {
    metadata: Option<EncodedMetadata>,
    path: Option<PathBuf>,
}

impl EarlyMetadata {
    pub fn take(&mut self) -> Option<EncodedMetadata> {
        self.path = None;
        self.metadata.take()
    }
}

impl Drop for EarlyMetadata {
    fn drop(&mut self) {
        drop(self.metadata.take());
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn write_early_metadata(tcx: TyCtxt<'_>) -> EarlyMetadata {
    let none = EarlyMetadata { metadata: None, path: None };
    let OutFileName::Real(path) = filename_for_metadata(tcx.sess, tcx.output_filenames(())) else {
        return none;
    };
    if !crate::rmeta::prepare_early_metadata(tcx) {
        return none;
    }
    EarlyMetadata { metadata: Some(encode_and_write_metadata(tcx)), path: Some(path) }
}

/// We use a temp directory here to avoid races between concurrent rustc processes,
/// such as builds in the same directory using the same filename for metadata while
/// building an `.rlib` (stomping over one another), or writing an `.rmeta` into a
/// directory being searched for `extern crate` (observing an incomplete file).
/// The returned path is the temporary file containing the complete metadata.
pub fn emit_wrapper_file(sess: &Session, data: &[u8], tmpdir: &Path, name: &str) -> PathBuf {
    let out_filename = tmpdir.join(name);
    let result = fs::write(&out_filename, data);

    if let Err(err) = result {
        sess.dcx().emit_fatal(FailedWriteError { filename: out_filename, err });
    }

    out_filename
}

pub fn encode_and_write_metadata(tcx: TyCtxt<'_>) -> EncodedMetadata {
    let out_filename = filename_for_metadata(tcx.sess, tcx.output_filenames(()));
    // To avoid races with another rustc process scanning the output directory,
    // we need to write the file somewhere else and atomically move it to its
    // final destination, with an `fs::rename` call. In order for the rename to
    // always succeed, the temporary file needs to be on the same filesystem,
    // which is why we create it inside the output directory specifically.
    let metadata_tmpdir = TempDirBuilder::new()
        .prefix("rmeta")
        .tempdir_in(out_filename.parent().unwrap_or_else(|| Path::new("")))
        .unwrap_or_else(|err| tcx.dcx().emit_fatal(FailedCreateTempdir { err }));
    let metadata_tmpdir = MaybeTempDir::new(metadata_tmpdir, tcx.sess.opts.cg.save_temps);
    let metadata_filename = metadata_tmpdir.as_ref().join("full.rmeta");
    let metadata_stub_filename = if !tcx.sess.opts.unstable_opts.embed_metadata
        && !tcx.crate_types().contains(&CrateType::ProcMacro)
    {
        Some(metadata_tmpdir.as_ref().join("stub.rmeta"))
    } else {
        None
    };

    if tcx.needs_metadata() {
        encode_metadata(tcx, &metadata_filename, metadata_stub_filename.as_deref());
    } else {
        // Always create a file at `metadata_filename`, even if we have nothing to write to it.
        // This simplifies the creation of the output `out_filename` when requested.
        std::fs::File::create(&metadata_filename).unwrap_or_else(|err| {
            tcx.dcx().emit_fatal(FailedCreateFile { filename: &metadata_filename, err });
        });
        if let Some(metadata_stub_filename) = &metadata_stub_filename {
            std::fs::File::create(metadata_stub_filename).unwrap_or_else(|err| {
                tcx.dcx().emit_fatal(FailedCreateFile { filename: &metadata_stub_filename, err });
            });
        }
    }

    let _prof_timer = tcx.sess.prof.generic_activity("write_crate_metadata");

    // If the user requests metadata as output, rename `metadata_filename`
    // to the expected output `out_filename`. The match above should ensure
    // this file always exists.
    let need_metadata_file = tcx.sess.opts.output_types.contains_key(&OutputType::Metadata);
    let (metadata_filename, metadata_tmpdir) = if need_metadata_file {
        let filename = match out_filename {
            OutFileName::Real(ref path) => {
                if let Err(err) = non_durable_rename(&metadata_filename, path) {
                    tcx.dcx().emit_fatal(FailedWriteError { filename: path.to_path_buf(), err });
                }
                path.clone()
            }
            OutFileName::Stdout => {
                if out_filename.is_tty() {
                    tcx.dcx().emit_err(BinaryOutputToTty);
                } else if let Err(err) = copy_to_stdout(&metadata_filename) {
                    tcx.dcx()
                        .emit_err(FailedCopyToStdout { filename: metadata_filename.clone(), err });
                }
                metadata_filename
            }
        };
        (filename, Some(metadata_tmpdir))
    } else {
        (metadata_filename, Some(metadata_tmpdir))
    };

    let public_api_hash_path =
        tcx.sess.opts.unstable_opts.public_api_hash.then(|| metadata_filename.clone());

    // Load metadata back to memory: codegen may need to include it in object files.
    let metadata =
        EncodedMetadata::from_path(metadata_filename, metadata_stub_filename, metadata_tmpdir)
            .unwrap_or_else(|err| {
                tcx.dcx().emit_fatal(FailedCreateEncodedMetadata { err });
            });

    if need_metadata_file && tcx.sess.opts.json_artifact_notifications {
        let public_api_hash = public_api_hash_path
            .as_deref()
            .and_then(|path| recover_public_api_hash(path, tcx.sess.cfg_version));
        tcx.dcx().emit_metadata_artifact_notification(
            out_filename.as_path(),
            public_api_hash.as_deref(),
        );
    }

    metadata
}

/// Recovers the public API hash from the metadata file that was actually written.
///
/// Reading the completed file, instead of returning the freshly computed hash from the encoder,
/// also covers the incremental path that copies a green metadata work product.
fn recover_public_api_hash(path: &Path, cfg_version: &'static str) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    // SAFETY: `path` names the completed metadata file owned by this compiler invocation. We only
    // read through the mapping and keep the file-backed mapping alive in `MetadataBlob`.
    let mmap = unsafe { Mmap::map(file).ok()? };
    if mmap.len() < METADATA_HEADER.len() + size_of::<u64>() || !mmap.starts_with(METADATA_HEADER) {
        return None;
    }

    let mut metadata = MetadataBlob::new(slice_owned(mmap, Deref::deref)).ok()?;
    metadata.check_compatibility(cfg_version, || false).ok()?;
    Some(metadata.get_header().hashes.public_hash.to_hex())
}

#[cfg(not(target_os = "linux"))]
pub fn non_durable_rename(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// This function attempts to bypass the auto_da_alloc heuristic implemented by some filesystems
/// such as btrfs and ext4. When renaming over a file that already exists then they will "helpfully"
/// write back the source file before committing the rename in case a developer forgot some of
/// the fsyncs in the open/write/fsync(file)/rename/fsync(dir) dance for atomic file updates.
///
/// To avoid triggering this heuristic we delete the destination first, if it exists.
/// The cost of an extra syscall is much lower than getting descheduled for the sync IO.
#[cfg(target_os = "linux")]
pub fn non_durable_rename(src: &Path, dst: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(dst);
    std::fs::rename(src, dst)
}

pub fn copy_to_stdout(from: &Path) -> io::Result<()> {
    let mut reader = fs::File::open_buffered(from)?;
    let mut stdout = io::stdout();
    io::copy(&mut reader, &mut stdout)?;
    Ok(())
}
