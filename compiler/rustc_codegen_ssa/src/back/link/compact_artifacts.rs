use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::temp_dir::MaybeTempDir;
use rustc_fs_util::TempDirBuilder;
use rustc_middle::middle::dependency_format::Linkage;
use rustc_session::Session;
use rustc_session::config::{CrateType, OutputFilenames};
use rustc_session::output::out_filename;

use crate::back::archive::{AddArchiveKind, ArchiveBuilderBuilder};
use crate::{CrateInfo, diagnostics};

/// Adapt compact Rust archives for existing system linkers. The returned owner keeps temporary
/// files alive until linking finishes, unless the user explicitly requests saved temps.
pub(super) fn materialize_compact_rlibs(
    sess: &Session,
    archive_builder_builder: &dyn ArchiveBuilderBuilder,
    crate_info: &mut CrateInfo,
    outputs: &OutputFilenames,
) -> Option<MaybeTempDir> {
    let crate_type = crate_info.crate_types.iter().copied().find(|t| *t != CrateType::Rlib)?;
    if sess.opts.unstable_opts.compact_artifacts {
        return None;
    }
    let output = out_filename(sess, crate_type, outputs, crate_info.local_crate_name);
    let mut directory: Option<MaybeTempDir> = None;
    for cnum in crate_info.used_crates.clone() {
        let needs_archive = crate_info.crate_types.iter().any(|crate_type| {
            *crate_type != CrateType::Rlib
                && crate_info
                    .dependency_formats
                    .get(crate_type)
                    .and_then(|formats| formats.get(cnum))
                    == Some(&Linkage::Static)
        });
        if !needs_archive {
            continue;
        }
        let Some(source) = crate_info.used_crate_source[&cnum].rlib.clone() else {
            continue;
        };
        if !rlib_contains_compact_objects(&source).unwrap_or_else(|error| {
            sess.dcx().emit_fatal(diagnostics::CompressedArtifactError {
                action: "inspect",
                path: source.clone(),
                error,
            });
        }) {
            continue;
        }
        let dir = directory.get_or_insert_with(|| {
            let dir = TempDirBuilder::new()
                .prefix("rustc-artifacts")
                .tempdir_in(output.parent().unwrap_or_else(|| Path::new(".")))
                .unwrap_or_else(|error| {
                    sess.dcx().emit_fatal(diagnostics::CreateTempDir { error })
                });
            MaybeTempDir::new(dir, sess.opts.cg.save_temps)
        });
        // Separate directories retain the exact archive basename, even when inputs from
        // different directories happen to have the same name.
        let parent = dir.as_ref().join(cnum.as_u32().to_string());
        let dest = parent.join(source.file_name().unwrap());
        let materialize = || -> io::Result<()> {
            fs::create_dir(&parent)?;
            let mut builder = archive_builder_builder.new_archive_builder(sess);
            builder.add_archive(&source, AddArchiveKind::Other)?;
            builder.build(&dest, None);
            Ok(())
        };
        materialize().unwrap_or_else(|error| {
            sess.dcx().emit_fatal(diagnostics::CompressedArtifactError {
                action: "materialize",
                path: source.clone(),
                error,
            });
        });
        Arc::make_mut(crate_info.used_crate_source.get_mut(&cnum).unwrap()).rlib = Some(dest);
    }
    directory
}

/// Inspect archive headers and member magic without decoding compact payloads.
/// Ordinary consumers (including rustdoc doctests) may not use the compact-capable linker.
fn rlib_contains_compact_objects(path: &Path) -> io::Result<bool> {
    use rustc_data_structures::compact_artifact as compact;

    let map = unsafe { Mmap::map(File::open(path)?)? };
    let archive = object::read::archive::ArchiveFile::parse(&*map).map_err(io::Error::other)?;
    for member in archive.members() {
        let member = member.map_err(io::Error::other)?;
        if member.is_thin() {
            let name = std::str::from_utf8(member.name()).map_err(io::Error::other)?;
            let mut file = File::open(path.parent().unwrap_or(Path::new(".")).join(name))?;
            let mut magic = [0; 8];
            match file.read_exact(&mut magic) {
                Ok(()) if magic == *compact::MAGIC => return Ok(true),
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {}
                Err(error) => return Err(error),
            }
        } else if member.data(&*map).map_err(io::Error::other)?.starts_with(compact::MAGIC) {
            return Ok(true);
        }
    }
    Ok(false)
}
