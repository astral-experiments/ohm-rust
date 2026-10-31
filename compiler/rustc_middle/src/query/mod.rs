use rustc_ast::tokenstream::TokenStream;
use rustc_span::Symbol;

pub use self::caches::{DefIdCache, DefaultCache, QueryCache, SingleCache, VecCache};
pub use self::calls::{TyCtxtAt, TyCtxtEnsureDone, TyCtxtEnsureOk, TyCtxtEnsureResult};
pub use self::into_query_key::IntoQueryKey;
pub use self::job::{
    ActiveKeyStatus, QueryCycle, QueryJob, QueryJobId, QueryLatch, QueryStackFrame, QueryState,
    QueryWaiter,
};
pub use self::keys::{LocalCrate, QueryKey};
pub use self::system::{QueryMode, QuerySystem, QueryVTable};
pub use crate::queries::Providers;

pub(crate) mod arena_cached;
mod caches;
pub(crate) mod calls;
pub mod erase;
mod into_query_key;
mod job;
mod keys;
pub(crate) mod modifiers;
pub mod on_disk_cache;
pub(crate) mod query_api;
mod system;

/// Expanded tokens and the virtual source files allocated by this macro, in parse order.
pub type ProcMacroExpansion<'tcx, E> = (Result<&'tcx TokenStream, E>, &'tcx [Symbol]);
