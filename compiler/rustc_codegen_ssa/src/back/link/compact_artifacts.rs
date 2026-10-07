use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::temp_dir::MaybeTempDir;
use rustc_fs_util::TempDirBuilder;
use rustc_middle::middle::dependency_format::Linkage;
use rustc_session::Session;
use rustc_session::config::{CrateType, OutputFilenames};
use rustc_session::output::out_filename;

use crate::back::archive::{AddArchiveKind, ArchiveBuilderBuilder};
use crate::{CompiledModules, CrateInfo, diagnostics};

/// Private ordinary inputs for consumers that do not understand compact objects.
/// Source archives and incremental inodes remain untouched.
pub(super) struct MaterializedInputs<'a> {
    sess: &'a Session,
    archive_builder_builder: &'a dyn ArchiveBuilderBuilder,
    parent: PathBuf,
    directory: Option<MaybeTempDir>,
    next_input: usize,
    original_objects: Vec<(usize, bool, PathBuf)>,
}

impl<'a> MaterializedInputs<'a> {
    pub(super) fn new(
        sess: &'a Session,
        archive_builder_builder: &'a dyn ArchiveBuilderBuilder,
        parent: Option<&Path>,
    ) -> Self {
        Self {
            sess,
            archive_builder_builder,
            parent: parent
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
            directory: None,
            next_input: 0,
            original_objects: Vec::new(),
        }
    }

    /// Return a conventional copy only when this input contains compact objects.
    pub(super) fn materialize(&mut self, path: &Path) -> Option<PathBuf> {
        self.try_materialize(path).unwrap_or_else(|error| {
            self.sess.dcx().emit_fatal(diagnostics::CompressedArtifactError {
                action: "materialize",
                path: path.to_path_buf(),
                error,
            });
        })
    }

    /// Restore output paths so normal cleanup applies to the original compiler temporaries.
    pub(super) fn restore_objects(&mut self, modules: &mut CompiledModules) {
        for (index, global_asm, path) in self.original_objects.drain(..) {
            let module = if index < modules.modules.len() {
                &mut modules.modules[index]
            } else {
                modules.allocator_module.as_mut().unwrap()
            };
            *if global_asm { &mut module.global_asm_object } else { &mut module.object } =
                Some(path);
        }
    }

    fn try_materialize(&mut self, path: &Path) -> io::Result<Option<PathBuf>> {
        use rustc_data_structures::compact_artifact as compact;

        let map = unsafe { Mmap::map(File::open(path)?)? };
        let object = map.starts_with(compact::MAGIC);
        if !object
            && (!(map.starts_with(b"!<arch>\n") || map.starts_with(b"!<thin>\n"))
                || !rlib_contains_compact_objects(path, &map)?)
        {
            return Ok(None);
        }
        if self.directory.is_none() {
            let dir = TempDirBuilder::new().prefix("rustc-artifacts").tempdir_in(&self.parent)?;
            self.directory = Some(MaybeTempDir::new(dir, self.sess.opts.cg.save_temps));
        }
        // Preserve basenames and archive selection, including when inputs from different
        // directories happen to share a filename.
        let parent = self.directory.as_ref().unwrap().as_ref().join(self.next_input.to_string());
        self.next_input += 1;
        fs::create_dir(&parent)?;
        let dest = parent.join(path.file_name().unwrap());
        if object {
            fs::write(&dest, compact::unpack_elf(&map)?)?;
        } else {
            let mut builder = self.archive_builder_builder.new_archive_builder(self.sess);
            builder.add_archive(path, AddArchiveKind::Other)?;
            builder.build(&dest, None);
        }
        Ok(Some(dest))
    }
}

/// Materialize local objects and dependencies only for a conventional native link.
/// The returned owner keeps every temporary input alive for the complete link operation.
pub(super) fn materialize_compact_inputs<'a>(
    sess: &'a Session,
    archive_builder_builder: &'a dyn ArchiveBuilderBuilder,
    compiled_modules: &mut CompiledModules,
    crate_info: &mut CrateInfo,
    outputs: &OutputFilenames,
) -> Option<MaterializedInputs<'a>> {
    let crate_type = crate_info.crate_types.iter().copied().find(|t| *t != CrateType::Rlib)?;
    if sess.opts.unstable_opts.compact_artifacts_linker {
        return None;
    }
    let output = out_filename(sess, crate_type, outputs, crate_info.local_crate_name);
    let mut inputs = MaterializedInputs::new(sess, archive_builder_builder, output.parent());
    // Normal ordinary codegen cannot reuse compact local objects: the storage mode is
    // fingerprinted. Link-only recipes can refer to objects produced in a different mode.
    if sess.opts.unstable_opts.compact_artifacts || sess.opts.unstable_opts.link_only {
        for (index, module) in compiled_modules
            .modules
            .iter_mut()
            .chain(&mut compiled_modules.allocator_module)
            .enumerate()
        {
            for (global_asm, slot) in
                [(false, &mut module.object), (true, &mut module.global_asm_object)]
            {
                if let Some(path) = slot
                    && let Some(ordinary) = inputs.materialize(path)
                {
                    inputs.original_objects.push((
                        index,
                        global_asm,
                        std::mem::replace(path, ordinary),
                    ));
                }
            }
        }
    }
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
        if let Some(dest) = inputs.materialize(&source) {
            Arc::make_mut(crate_info.used_crate_source.get_mut(&cnum).unwrap()).rlib = Some(dest);
        }
    }
    Some(inputs)
}

/// Debuggers follow split-DWARF paths independently of the native linker. Replace only the
/// retained output inode so the incremental cache can keep its immutable compact object.
pub(super) fn materialize_debug_objects(sess: &Session, modules: &CompiledModules) {
    use rustc_data_structures::compact_artifact as compact;

    for module in modules.modules.iter().chain(&modules.allocator_module) {
        for path in [&module.object, &module.global_asm_object].into_iter().flatten() {
            let materialize = || -> io::Result<()> {
                let bytes = fs::read(path)?;
                if bytes.starts_with(compact::MAGIC) {
                    compact::atomic_write(path, &compact::unpack_elf(&bytes)?)?;
                }
                Ok(())
            };
            materialize().unwrap_or_else(|error| {
                sess.dcx().emit_fatal(diagnostics::CompressedArtifactError {
                    action: "materialize debug object",
                    path: path.clone(),
                    error,
                });
            });
        }
    }
}

/// Inspect archive headers and member magic without decoding compact payloads.
/// Ordinary consumers (including rustdoc doctests) may not use the compact-capable linker.
fn rlib_contains_compact_objects(path: &Path, data: &[u8]) -> io::Result<bool> {
    use rustc_data_structures::compact_artifact as compact;

    let archive = object::read::archive::ArchiveFile::parse(data).map_err(io::Error::other)?;
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
        } else if member.data(data).map_err(io::Error::other)?.starts_with(compact::MAGIC) {
            return Ok(true);
        }
    }
    Ok(false)
}
