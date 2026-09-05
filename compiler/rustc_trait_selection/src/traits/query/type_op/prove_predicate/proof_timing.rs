//! Opt-in elapsed times for canonical TypeOp calls, including query cache hits.
//!
//! Depth-zero intervals are additive within a worker. Inclusive intervals overlap when
//! TypeOps nest, and include a child's diagnostic key preparation and counter recording.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, ParamEnvAnd, Ty, TyCtxt, TypeVisitableExt};

use super::ProvePredicate;

const MAX_KEYS: usize = 65_536;
const MAX_DEPTH: usize = 2;
const MAX_WIDTH: usize = 4;
static OUTPUT: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var_os("OHM_PROOF_TIMING_DIR").map(PathBuf::from));
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static COUNTS: RefCell<Option<Counts>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Default)]
struct Metrics {
    calls: u64,
    inclusive_ns: u128,
    outer_calls: u64,
    outer_ns: u128,
}

impl Metrics {
    fn add(&mut self, value: Self) {
        self.calls += value.calls;
        self.inclusive_ns += value.inclusive_ns;
        self.outer_calls += value.outer_calls;
        self.outer_ns += value.outer_ns;
    }
}

struct Counts {
    output: PathBuf,
    id: u64,
    keys: BTreeMap<String, Metrics>,
    total: Metrics,
    overflow: Metrics,
    limited_shapes: u64,
    depth: usize,
    max_depth: usize,
    nesting_errors: u64,
}

impl Counts {
    fn new(output: PathBuf) -> Self {
        let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let _ = writeln!(
            io::stderr().lock(),
            "OHM_PROOF_TIMING_BEGIN\t{}\t{id}",
            std::process::id()
        );
        Self {
            output,
            id,
            keys: BTreeMap::new(),
            total: Metrics::default(),
            overflow: Metrics::default(),
            limited_shapes: 0,
            depth: 0,
            max_depth: 0,
            nesting_errors: 0,
        }
    }

    fn dump(&self) -> io::Result<()> {
        let stem = format!("proof-timing-{}-{}", std::process::id(), self.id);
        let partial = self.output.join(format!("{stem}.partial"));
        let complete = self.output.join(format!("{stem}.tsv"));
        let mut out = BufWriter::new(File::create_new(&partial)?);
        writeln!(
            out,
            "ohm-proof-timing-v1\t{MAX_KEYS}\t{MAX_DEPTH}\t{MAX_WIDTH}"
        )?;
        for (key, metrics) in &self.keys {
            writeln!(
                out,
                "row\t{}\t{}\t{}\t{}\t{key}",
                metrics.calls, metrics.inclusive_ns, metrics.outer_calls, metrics.outer_ns
            )?;
        }
        writeln!(
            out,
            "complete\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.total.calls,
            self.keys.len(),
            self.overflow.calls,
            self.limited_shapes,
            self.total.inclusive_ns,
            self.total.outer_calls,
            self.total.outer_ns,
            self.overflow.inclusive_ns,
            self.overflow.outer_calls,
            self.overflow.outer_ns,
            self.max_depth,
            self.depth,
            self.nesting_errors
        )?;
        out.flush()?;
        fs::rename(partial, complete)
    }
}

impl Drop for Counts {
    fn drop(&mut self) {
        // Only owned data remains after the worker's compiler context has been dropped.
        if let Err(error) = self.dump() {
            let _ = writeln!(
                io::stderr().lock(),
                "OHM_PROOF_TIMING_DUMP_ERROR\t{}\t{}\t{error}",
                std::process::id(),
                self.id
            );
        }
    }
}

pub(crate) struct Timer {
    key: String,
    limited: bool,
    depth: usize,
    started: Instant,
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Capture the endpoint before borrowing statistics or recording this call.
        let finished = Instant::now();
        let elapsed = finished.duration_since(self.started).as_nanos();
        let metrics = Metrics {
            calls: 1,
            inclusive_ns: elapsed,
            outer_calls: u64::from(self.depth == 1),
            outer_ns: if self.depth == 1 { elapsed } else { 0 },
        };
        COUNTS.with(|cell| {
            let mut counts = cell.borrow_mut();
            let counts = counts.as_mut().unwrap();
            counts.nesting_errors += u64::from(counts.depth != self.depth);
            counts.depth = counts.depth.saturating_sub(1);
            counts.total.add(metrics);
            counts.limited_shapes += u64::from(self.limited);
            if let Some(value) = counts.keys.get_mut(&self.key) {
                value.add(metrics);
            } else if counts.keys.len() < MAX_KEYS {
                counts.keys.insert(std::mem::take(&mut self.key), metrics);
            } else {
                counts.overflow.add(metrics);
            }
        });
    }
}

fn start(output: &PathBuf, key: String, limited: bool) -> Timer {
    let depth = COUNTS.with(|cell| {
        let mut counts = cell.borrow_mut();
        let counts = counts.get_or_insert_with(|| Counts::new(output.clone()));
        counts.depth += 1;
        counts.max_depth = counts.max_depth.max(counts.depth);
        counts.depth
    });
    // All key formatting, compiler metadata access, and initial setup precede the clock.
    Timer {
        key,
        limited,
        depth,
        started: Instant::now(),
    }
}

pub(crate) fn start_other<Q>() -> Option<Timer> {
    let output = OUTPUT.as_ref()?;
    let key = format!("{}\t-\t-\t-\t-\t-", escape(std::any::type_name::<Q>()));
    Some(start(output, key, false))
}

pub(crate) fn start_prove<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: &ParamEnvAnd<'tcx, ProvePredicate<'tcx>>,
) -> Option<Timer> {
    let output = OUTPUT.as_ref()?;
    let (key, limited) = prepare_prove_key(tcx, key);
    Some(start(output, key, limited))
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn nominal(tcx: TyCtxt<'_>, def_id: DefId) -> String {
    // DefPath reads stored definition keys; unlike def_path_str, it never prints an impl's type.
    format!(
        "{}:{}:local={}:{}",
        def_id.krate.as_u32(),
        def_id.index.as_u32(),
        def_id.is_local(),
        tcx.def_path(def_id).to_string_no_crate_verbose(),
    )
}

fn arguments<'tcx>(
    tcx: TyCtxt<'tcx>,
    args: ty::GenericArgsRef<'tcx>,
    depth: usize,
    limited: &mut bool,
) -> String {
    let mut kinds = [0; 3];
    let mut shown = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let slot = match arg.kind() {
            ty::GenericArgKind::Lifetime(_) => 0,
            ty::GenericArgKind::Type(_) => 1,
            ty::GenericArgKind::Const(_) => 2,
        };
        kinds[slot] += 1;
        if index >= MAX_WIDTH {
            *limited = true;
            continue;
        }
        shown.push(match arg.kind() {
            ty::GenericArgKind::Lifetime(_) => "L".to_owned(),
            ty::GenericArgKind::Const(_) => "C".to_owned(),
            ty::GenericArgKind::Type(ty) if depth < MAX_DEPTH => {
                format!("T={}", shape(tcx, ty, depth + 1, limited))
            }
            ty::GenericArgKind::Type(_) => {
                *limited = true;
                "T=...".to_owned()
            }
        });
    }
    format!(
        "L{}T{}C{}[{}]",
        kinds[0],
        kinds[1],
        kinds[2],
        shown.join(",")
    )
}

fn shape<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>, depth: usize, limited: &mut bool) -> String {
    let mut child = |ty| {
        if depth < MAX_DEPTH {
            shape(tcx, ty, depth + 1, limited)
        } else {
            *limited = true;
            "...".to_owned()
        }
    };
    match *ty.kind() {
        ty::Adt(def, args) => {
            format!(
                "Adt({};{})",
                nominal(tcx, def.did().into()),
                arguments(tcx, args, depth, limited)
            )
        }
        ty::Ref(_, ty, mutability) => format!("Ref({mutability:?};{})", child(ty)),
        ty::RawPtr(ty, mutability) => format!("RawPtr({mutability:?};{})", child(ty)),
        ty::Array(ty, _) => format!("Array({};C)", child(ty)),
        ty::Slice(ty) => format!("Slice({})", child(ty)),
        ty::Pat(ty, _) => format!("Pat({})", child(ty)),
        ty::Tuple(types) => {
            let items = types
                .iter()
                .take(MAX_WIDTH)
                .map(&mut child)
                .collect::<Vec<_>>();
            if types.len() > MAX_WIDTH {
                *limited = true;
            }
            format!("Tuple({};{})", types.len(), items.join(","))
        }
        ty::FnDef(_, args) => format!("FnDef({})", arguments(tcx, args, depth, limited)),
        ty::Closure(_, args) => format!("Closure({})", arguments(tcx, args, depth, limited)),
        ty::CoroutineClosure(_, args) => {
            format!("CoroutineClosure({})", arguments(tcx, args, depth, limited))
        }
        ty::Coroutine(_, args) => format!("Coroutine({})", arguments(tcx, args, depth, limited)),
        ty::CoroutineWitness(_, args) => {
            format!("CoroutineWitness({})", arguments(tcx, args, depth, limited))
        }
        ty::Bool => "Bool".to_owned(),
        ty::Char => "Char".to_owned(),
        ty::Int(_) => "Int".to_owned(),
        ty::Uint(_) => "Uint".to_owned(),
        ty::Float(_) => "Float".to_owned(),
        ty::Foreign(_) => "Foreign".to_owned(),
        ty::Str => "Str".to_owned(),
        ty::FnPtr(..) => "FnPtr".to_owned(),
        ty::UnsafeBinder(_) => "UnsafeBinder".to_owned(),
        ty::Dynamic(..) => "Dynamic".to_owned(),
        ty::Never => "Never".to_owned(),
        ty::Alias(..) => "Alias".to_owned(),
        ty::Param(_) => "Param".to_owned(),
        ty::Bound(..) => "Bound".to_owned(),
        ty::Placeholder(_) => "Placeholder".to_owned(),
        ty::Infer(_) => "Infer".to_owned(),
        ty::Error(_) => "Error".to_owned(),
    }
}

fn prepare_prove_key<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: &ParamEnvAnd<'tcx, ProvePredicate<'tcx>>,
) -> (String, bool) {
    let predicate = key.value.predicate;
    let mut trait_name = "-".to_owned();
    let mut subject = None;
    let mut generic_args = 0;
    let mut positive_caller_bound = false;
    let kind = match predicate.kind().skip_binder() {
        ty::PredicateKind::Clause(clause) => match clause {
            ty::ClauseKind::Trait(data) => {
                trait_name = nominal(tcx, data.def_id().into());
                subject = Some(data.self_ty());
                if data.polarity == ty::PredicatePolarity::Positive {
                    generic_args = data.trait_ref.args.len();
                    positive_caller_bound = key.param_env.caller_bounds().iter().any(|clause| {
                        matches!(
                            clause.kind().skip_binder(),
                            ty::ClauseKind::Trait(bound)
                                if bound.polarity == ty::PredicatePolarity::Positive
                                    && bound.def_id() == data.def_id()
                        )
                    });
                }
                match data.polarity {
                    ty::PredicatePolarity::Positive => "TraitPositive",
                    ty::PredicatePolarity::Negative => "TraitNegative",
                }
            }
            ty::ClauseKind::HostEffect(data) => {
                trait_name = nominal(tcx, data.def_id().into());
                subject = Some(data.self_ty());
                "HostEffect"
            }
            ty::ClauseKind::Projection(data) => {
                trait_name = nominal(tcx, data.trait_def_id(tcx).into());
                subject = Some(data.self_ty());
                "Projection"
            }
            ty::ClauseKind::WellFormed(term) => match term.kind() {
                ty::TermKind::Ty(ty) => {
                    subject = Some(ty);
                    "WellFormedType"
                }
                ty::TermKind::Const(_) => "WellFormedConst",
            },
            ty::ClauseKind::TypeOutlives(data) => {
                subject = Some(data.0);
                "TypeOutlives"
            }
            ty::ClauseKind::ConstArgHasType(_, ty) => {
                subject = Some(ty);
                "ConstArgHasType"
            }
            ty::ClauseKind::RegionOutlives(_) => "RegionOutlives",
            ty::ClauseKind::ConstEvaluatable(_) => "ConstEvaluatable",
            ty::ClauseKind::UnstableFeature(_) => "UnstableFeature",
        },
        ty::PredicateKind::DynCompatible(def_id) => {
            trait_name = nominal(tcx, def_id.into());
            "DynCompatible"
        }
        ty::PredicateKind::Subtype(_) => "Subtype",
        ty::PredicateKind::Coerce(_) => "Coerce",
        ty::PredicateKind::ConstEquate(..) => "ConstEquate",
        ty::PredicateKind::Ambiguous => "Ambiguous",
        ty::PredicateKind::NormalizesTo(_) => "NormalizesTo",
    };
    let mut limited = false;
    let subject = subject.map_or_else(|| "-".to_owned(), |ty| shape(tcx, ty, 0, &mut limited));
    // Finish all compiler metadata access before mutably borrowing thread-local counters.
    let key = format!(
        "ProvePredicate\t{}\t{}\t{}\tb{}a{}i{}e{}x{}g{}p{}\t{}",
        kind,
        escape(&trait_name),
        escape(&subject),
        predicate.kind().bound_vars().len(),
        u8::from(predicate.has_aliases()),
        u8::from(predicate.has_non_region_infer()),
        u8::from(predicate.references_error()),
        u8::from(predicate.has_escaping_bound_vars()),
        generic_args,
        u8::from(positive_caller_bound),
        key.param_env.caller_bounds().len(),
    );
    (key, limited)
}
