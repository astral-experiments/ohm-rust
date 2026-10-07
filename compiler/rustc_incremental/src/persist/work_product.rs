//! Functions for saving and removing intermediate [work products].
//!
//! [work products]: WorkProduct

use std::fs as std_fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rustc_data_structures::unord::UnordMap;
use rustc_fs_util::link_or_copy;
use rustc_middle::dep_graph::{WorkProduct, WorkProductId};
use rustc_session::{IncrCompSession, Session};
use tracing::debug;

use crate::diagnostics;
use crate::persist::fs::*;

/// Copies a CGU work product to the incremental compilation directory, so next compilation can
/// find and reuse it.
///
/// Panics when incr comp is disabled.
pub fn copy_cgu_workproduct_to_incr_comp_cache_dir(
    sess: &Session,
    incr_comp_session: &IncrCompSession,
    cgu_name: &str,
    files: &[(&'static str, &Path)],
    known_links: &[PathBuf],
) -> (WorkProductId, WorkProduct) {
    debug!(?cgu_name, ?files);
    assert!(sess.opts.incremental.is_some());

    let mut saved_files = UnordMap::default();
    for (ext, path) in files {
        let file_name = format!("{cgu_name}.{ext}");
        let path_in_incr_dir = in_incr_comp_dir_sess(incr_comp_session, &file_name);
        if known_links.contains(&path_in_incr_dir) {
            let _ = saved_files.insert(ext.to_string(), file_name);
            continue;
        }
        if *ext == "o" && sess.opts.unstable_opts.compact_artifacts {
            use rustc_data_structures::compact_artifact as compact;
            let result = (|| -> std::io::Result<()> {
                let options = sess.opts.unstable_opts.compact_artifact_options();
                let bytes = std_fs::read(path)?;
                // Linker-plugin LTO emits raw LLVM bitcode in `.o` files. Preserve it
                // for the plugin instead of treating the filename as proof of ELF.
                if bytes.starts_with(b"\x7fELF") {
                    // Publish a new inode: this output may already share an immutable
                    // incremental work product with another compilation or worktree.
                    compact::atomic_write(path, &compact::pack_elf(&bytes, options)?)?;
                }
                compact::link_object(path, &path_in_incr_dir)
            })();
            match result {
                Ok(()) => {
                    let _ = saved_files.insert(ext.to_string(), file_name);
                }
                Err(err) => {
                    sess.dcx().emit_warn(diagnostics::CopyWorkProductToCache {
                        from: path,
                        to: &path_in_incr_dir,
                        err,
                    });
                }
            }
            continue;
        }
        match link_or_copy(path, &path_in_incr_dir) {
            Ok(_) => {
                let _ = saved_files.insert(ext.to_string(), file_name);
            }
            Err(err) => {
                sess.dcx().emit_warn(diagnostics::CopyWorkProductToCache {
                    from: path,
                    to: &path_in_incr_dir,
                    err,
                });
            }
        }
    }

    let work_product = WorkProduct { cgu_name: cgu_name.to_string(), saved_files };
    debug!(?work_product);
    let work_product_id = WorkProductId::from_cgu_name(cgu_name);
    (work_product_id, work_product)
}

/// Removes files for a given work product.
pub(crate) fn delete_workproduct_files(
    sess: &Session,
    incr_comp_session: &IncrCompSession,
    work_product: &WorkProduct,
) {
    for (_, path) in work_product.saved_files.items().into_sorted_stable_ord() {
        let path = in_incr_comp_dir_sess(incr_comp_session, path);
        if let Err(err) = std_fs::remove_file(&path)
            && err.kind() != ErrorKind::NotFound
        {
            sess.dcx().emit_warn(diagnostics::DeleteWorkProduct { path: &path, err });
        }
    }
}
