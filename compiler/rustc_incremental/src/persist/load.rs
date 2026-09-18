//! Code to load the dep-graph from files.

use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, io};

use rustc_data_structures::memmap::Mmap;
use rustc_data_structures::unord::UnordMap;
use rustc_hashes::Hash64;
use rustc_middle::dep_graph::{DepGraph, SerializedDepGraph, WorkProductMap};
use rustc_middle::query::on_disk_cache::OnDiskCache;
use rustc_serialize::opaque::{FileEncoder, MemDecoder};
use rustc_serialize::{Decodable, Encodable};
use rustc_session::config::IncrementalStateAssertion;
use rustc_session::{Session, StableCrateId};
use rustc_span::Symbol;
use tracing::{debug, warn};

use super::data::*;
use super::fs::*;
use super::{file_format, work_product};
use crate::diagnostics;
use crate::persist::file_format::{OpenFile, OpenFileError};

#[derive(Debug)]
/// Represents the result of an attempt to load incremental compilation data.
enum LoadResult {
    /// Loading was successful.
    Ok { prev_graph: Arc<SerializedDepGraph>, prev_work_products: WorkProductMap },
    /// The file either didn't exist or was produced by an incompatible compiler version.
    DataOutOfDate,
    /// Loading failed due to an unexpected I/O error.
    IoError { path: PathBuf, err: io::Error },
}

fn delete_dirty_work_product(sess: &Session, swp: SerializedWorkProduct) {
    debug!("delete_dirty_work_product({:?})", swp);
    work_product::delete_workproduct_files(sess, &swp.work_product);
}

pub(super) fn flat_dep_graph_enabled() -> bool {
    env::var_os("RUSTC_OHM_FLAT_DEP_GRAPH").is_some_and(|value| value == "1")
}

fn flat_dep_graph_trace(event: &str) {
    if env::var_os("RUSTC_OHM_FLAT_DEP_GRAPH_TRACE").is_some_and(|value| value == "1") {
        eprintln!("ohm-flat-dep-graph\t{event}");
    }
}

/// Produces the next generation's optional shareable representation without changing the stock
/// files. Snapshot errors are deliberately non-fatal; a later compiler still has its stock graph.
pub(super) fn write_flat_dep_graph(sess: &Session) {
    let result = (|| {
        let path = dep_graph_path(sess);
        let OpenFile { mmap, start_pos } = file_format::open_incremental_file(sess, &path).ok()?;
        let generation = SerializedDepGraph::snapshot_generation(&mmap);
        let mut decoder = MemDecoder::new(&mmap, start_pos).ok()?;
        if Hash64::decode(&mut decoder) != sess.opts.dep_tracking_hash(false) {
            return None;
        }
        let graph = SerializedDepGraph::decode(&mut decoder, &sess.prof);
        let staging = staging_flat_dep_graph_path(sess);
        let destination = flat_dep_graph_path(sess);
        if let Err(error) = fs::remove_file(&staging)
            && error.kind() != io::ErrorKind::NotFound
        {
            return None;
        }
        let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&staging).ok()?;
        graph.write_snapshot(generation, &mut output).ok()?;
        drop(output);
        // The destination may be cloned or linked from the previous completed session. Never
        // modify it in place: readers of either generation must see immutable backing storage.
        #[cfg(windows)]
        if let Err(error) = fs::remove_file(&destination)
            && error.kind() != io::ErrorKind::NotFound
        {
            return None;
        }
        fs::rename(&staging, &destination).ok()?;
        Some(())
    })();
    if result.is_some() {
        flat_dep_graph_trace("written");
    } else {
        let _ = fs::remove_file(staging_flat_dep_graph_path(sess));
        flat_dep_graph_trace("write-failed");
    }
}

fn load_dep_graph(sess: &Session) -> LoadResult {
    assert!(sess.opts.incremental.is_some());

    let _timer = sess.prof.generic_activity("incr_comp_prepare_load_dep_graph");

    // Calling `sess.incr_comp_session_dir()` will panic if `sess.opts.incremental.is_none()`.
    // Fortunately, we just checked that this isn't the case.
    let path = dep_graph_path(sess);
    let expected_hash = sess.opts.dep_tracking_hash(false);

    let mut prev_work_products = UnordMap::default();

    // If we are only building with -Zquery-dep-graph but without an actual
    // incr. comp. session directory, we skip this. Otherwise we'd fail
    // when trying to load work products.
    if sess.incr_comp_session_dir_opt().is_some() {
        let work_products_path = work_products_path(sess);

        if let Ok(OpenFile { mmap, start_pos }) =
            file_format::open_incremental_file(sess, &work_products_path)
        {
            // Decode the list of work_products
            let Ok(mut work_product_decoder) = MemDecoder::new(&mmap[..], start_pos) else {
                sess.dcx().emit_warn(diagnostics::CorruptFile { path: &work_products_path });
                return LoadResult::DataOutOfDate;
            };
            let work_products: Vec<SerializedWorkProduct> =
                Decodable::decode(&mut work_product_decoder);

            for swp in work_products {
                let all_files_exist = swp.work_product.saved_files.items().all(|(_, path)| {
                    let exists = in_incr_comp_dir_sess(sess, path).exists();
                    if !exists && sess.opts.unstable_opts.incremental_info {
                        eprintln!("incremental: could not find file for work product: {path}",);
                    }
                    exists
                });

                if all_files_exist {
                    debug!("reconcile_work_products: all files for {:?} exist", swp);
                    prev_work_products.insert(swp.id, swp.work_product);
                } else {
                    debug!("reconcile_work_products: some file for {:?} does not exist", swp);
                    delete_dirty_work_product(sess, swp);
                }
            }
        }
    }

    let _prof_timer = sess.prof.generic_activity("incr_comp_load_dep_graph");

    match file_format::open_incremental_file(sess, &path) {
        Err(OpenFileError::NotFoundOrHeaderMismatch) => LoadResult::DataOutOfDate,
        Err(OpenFileError::IoError { err }) => LoadResult::IoError { path: path.to_owned(), err },
        Ok(OpenFile { mmap, start_pos }) => {
            let Ok(mut decoder) = MemDecoder::new(&mmap, start_pos) else {
                sess.dcx().emit_warn(diagnostics::CorruptFile { path: &path });
                return LoadResult::DataOutOfDate;
            };
            let prev_commandline_args_hash = Hash64::decode(&mut decoder);

            if prev_commandline_args_hash != expected_hash {
                if sess.opts.unstable_opts.incremental_info {
                    eprintln!(
                        "[incremental] completely ignoring cache because of \
                                    differing commandline arguments"
                    );
                }
                // We can't reuse the cache, purge it.
                debug!("load_dep_graph_new: differing commandline arg hashes");

                // No need to do any further work
                return LoadResult::DataOutOfDate;
            }

            let prev_graph = if flat_dep_graph_enabled() {
                let _timer = sess.prof.generic_activity("incr_comp_load_flat_dep_graph");
                let snapshot = fs::File::open(flat_dep_graph_path(sess))
                    .ok()
                    // SAFETY: Completed generations are immutable, and stock save uses a new
                    // file rather than overwriting an earlier generation's mapped inode.
                    .and_then(|file| unsafe { Mmap::map(file) }.ok())
                    .and_then(|snapshot| {
                        let generation = SerializedDepGraph::snapshot_generation(&mmap);
                        SerializedDepGraph::from_snapshot(snapshot, generation, &sess.prof)
                    });
                if let Some(graph) = snapshot {
                    flat_dep_graph_trace("loaded");
                    graph
                } else {
                    flat_dep_graph_trace("fallback");
                    SerializedDepGraph::decode(&mut decoder, &sess.prof)
                }
            } else {
                SerializedDepGraph::decode(&mut decoder, &sess.prof)
            };

            LoadResult::Ok { prev_graph, prev_work_products }
        }
    }
}

/// Attempts to load the query result cache from disk
///
/// If we are not in incremental compilation mode, returns `None`.
/// Otherwise, tries to load the query result cache from disk,
/// creating an empty cache if it could not be loaded.
pub fn load_query_result_cache(sess: &Session) -> Option<OnDiskCache> {
    if sess.opts.incremental.is_none() {
        return None;
    }

    let _prof_timer = sess.prof.generic_activity("incr_comp_load_query_result_cache");

    let path = query_cache_path(sess);
    match file_format::open_incremental_file(sess, &path) {
        Ok(OpenFile { mmap, start_pos }) => {
            let cache = OnDiskCache::new(sess, mmap, start_pos).unwrap_or_else(|()| {
                sess.dcx().emit_warn(diagnostics::CorruptFile { path: &path });
                OnDiskCache::new_empty()
            });
            Some(cache)
        }
        Err(OpenFileError::NotFoundOrHeaderMismatch | OpenFileError::IoError { .. }) => {
            Some(OnDiskCache::new_empty())
        }
    }
}

/// Emits a fatal error if the assertion in `-Zassert-incr-state` doesn't match
/// the outcome of trying to load previous-session state.
fn maybe_assert_incr_state(sess: &Session, load_result: &LoadResult) {
    // Return immediately if there's nothing to assert.
    let Some(assertion) = sess.opts.unstable_opts.assert_incr_state else { return };

    // Match exhaustively to make sure we don't miss any cases.
    let loaded = match load_result {
        LoadResult::Ok { .. } => true,
        LoadResult::DataOutOfDate | LoadResult::IoError { .. } => false,
    };

    match assertion {
        IncrementalStateAssertion::Loaded => {
            if !loaded {
                sess.dcx().emit_fatal(diagnostics::AssertLoaded);
            }
        }
        IncrementalStateAssertion::NotLoaded => {
            if loaded {
                sess.dcx().emit_fatal(diagnostics::AssertNotLoaded)
            }
        }
    }
}

/// Loads the previous session's dependency graph from disk if possible, and
/// sets up streaming output for the current session's dep graph data into an
/// incremental session directory.
///
/// In non-incremental mode, a dummy dep graph is returned immediately.
pub fn setup_dep_graph(
    sess: &Session,
    crate_name: Symbol,
    stable_crate_id: StableCrateId,
) -> DepGraph {
    if sess.opts.incremental.is_none() {
        return DepGraph::new_disabled();
    }

    // `load_dep_graph` can only be called after `prepare_session_directory`.
    prepare_session_directory(sess, crate_name, stable_crate_id);
    // Try to load the previous session's dep graph and work products.
    let load_result = load_dep_graph(sess);

    sess.time("incr_comp_garbage_collect_session_directories", || {
        if let Err(e) = garbage_collect_session_directories(sess) {
            warn!(
                "Error while trying to garbage collect incremental compilation \
                cache directory: {e}",
            );
        }
    });

    // Emit a fatal error if `-Zassert-incr-state` is present and unsatisfied.
    maybe_assert_incr_state(sess, &load_result);

    let (prev_graph, prev_work_products) = match load_result {
        LoadResult::IoError { path, err } => {
            sess.dcx().emit_warn(diagnostics::LoadDepGraph { path, err });
            Default::default()
        }
        LoadResult::DataOutOfDate => {
            if let Err(err) = delete_all_session_dir_contents(sess) {
                sess.dcx()
                    .emit_err(diagnostics::DeleteIncompatible { path: dep_graph_path(sess), err });
            }
            Default::default()
        }
        LoadResult::Ok { prev_graph, prev_work_products } => (prev_graph, prev_work_products),
    };

    // Stream the dep-graph to an alternate file, to avoid overwriting anything in case of errors.
    let path_buf = staging_dep_graph_path(sess);

    let mut encoder = FileEncoder::new(&path_buf).unwrap_or_else(|err| {
        // We're in incremental mode but couldn't set up streaming output of the dep graph.
        // Exit immediately instead of continuing in an inconsistent and untested state.
        sess.dcx().emit_fatal(diagnostics::CreateDepGraph { path: &path_buf, err })
    });

    file_format::write_file_header(&mut encoder, sess);

    // First encode the commandline arguments hash
    sess.opts.dep_tracking_hash(false).encode(&mut encoder);

    DepGraph::new(sess, prev_graph, prev_work_products, encoder)
}
