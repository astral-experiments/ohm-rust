//! Opt-in counts of the actual C31 local TypeOp decisions.
//!
//! Preexisting trivial-WF and Sized fast paths do not reach this hook. The recorder
//! holds owned strings only, and never repeats WF generation or a solver query.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hir::def_id::DefId;
use rustc_middle::ty::{self, Flags, ParamEnvAnd, Ty, TyCtxt};

use super::{InputFailure, ProvePredicate};

const MAX_KEYS: usize = 65_536;
const MAX_DEPTH: usize = 2;
const MAX_WIDTH: usize = 4;
static OUTPUT: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var_os("OHM_LOCAL_WF_COVERAGE_DIR").map(PathBuf::from));
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static COUNTS: RefCell<Option<Counts>> = const { RefCell::new(None) };
}

struct Counts {
    output: PathBuf,
    id: u64,
    keys: BTreeMap<String, u64>,
    started: u64,
    finished: u64,
    overflow: u64,
    limited: u64,
    unfinished: u64,
    active: u64,
}

impl Counts {
    fn new(output: PathBuf) -> Self {
        let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let _ = writeln!(
            io::stderr().lock(),
            "OHM_LOCAL_WF_COVERAGE_BEGIN\t{}\t{id}",
            std::process::id()
        );
        Self {
            output,
            id,
            keys: BTreeMap::new(),
            started: 0,
            finished: 0,
            overflow: 0,
            limited: 0,
            unfinished: 0,
            active: 0,
        }
    }

    fn dump(&self) -> io::Result<()> {
        let stem = format!("local-wf-coverage-{}-{}", std::process::id(), self.id);
        let partial = self.output.join(format!("{stem}.partial"));
        let complete = self.output.join(format!("{stem}.tsv"));
        let mut out = BufWriter::new(File::create_new(&partial)?);
        writeln!(out, "ohm-local-wf-coverage-v1\t{MAX_KEYS}\t{MAX_DEPTH}\t{MAX_WIDTH}")?;
        for (key, count) in &self.keys {
            writeln!(out, "row\t{count}\t{key}")?;
        }
        writeln!(
            out,
            "complete\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.started,
            self.finished,
            self.keys.len(),
            self.overflow,
            self.limited,
            self.unfinished,
            self.active,
            self.keys.values().sum::<u64>()
        )?;
        out.flush()?;
        fs::rename(partial, complete)
    }
}

impl Drop for Counts {
    fn drop(&mut self) {
        // All compiler contexts may already be gone; only owned counter data remains.
        if let Err(error) = self.dump() {
            let _ = writeln!(
                io::stderr().lock(),
                "OHM_LOCAL_WF_COVERAGE_DUMP_ERROR\t{}\t{}\t{error}",
                std::process::id(),
                self.id
            );
        }
    }
}

struct Recorder {
    root: String,
    outcome: &'static str,
    reason: &'static str,
    failure: String,
    limited: bool,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let key = format!("{}\t{}\t{}\t{}", self.root, self.outcome, self.reason, self.failure);
        COUNTS.with(|cell| {
            let mut counts = cell.borrow_mut();
            let counts = counts.as_mut().unwrap();
            counts.finished += 1;
            counts.active -= 1;
            counts.limited += u64::from(self.limited);
            counts.unfinished += u64::from(self.outcome == "unfinished");
            if let Some(value) = counts.keys.get_mut(&key) {
                *value += 1;
            } else if counts.keys.len() < MAX_KEYS {
                counts.keys.insert(key, 1);
            } else {
                counts.overflow += 1;
            }
        });
    }
}

fn start(output: &PathBuf, root: String, limited: bool) -> Recorder {
    COUNTS.with(|cell| {
        let mut counts = cell.borrow_mut();
        let counts = counts.get_or_insert_with(|| Counts::new(output.clone()));
        counts.started += 1;
        counts.active += 1;
    });
    Recorder {
        root,
        outcome: "unfinished",
        reason: "unfinished",
        failure: "-\t-\t-\t-\t-\t-".to_owned(),
        limited,
    }
}

pub(super) struct Coverage(Option<Recorder>);

impl Coverage {
    pub(super) fn start<'tcx>(
        tcx: TyCtxt<'tcx>,
        key: &ParamEnvAnd<'tcx, ProvePredicate<'tcx>>,
    ) -> Self {
        let Some(output) = OUTPUT.as_ref() else { return Self(None) };
        let mut limited = false;
        let root = format!(
            "{}\t{}",
            predicate_fields(tcx, key.value.predicate, &mut limited),
            key.param_env.caller_bounds().len()
        );
        // Every compiler metadata read precedes the mutable thread-local counter borrow.
        Self(Some(start(output, root, limited)))
    }

    pub(super) fn accept(&mut self, reason: &'static str) {
        if let Some(record) = &mut self.0 {
            record.outcome = "accepted";
            record.reason = reason;
        }
    }

    pub(super) fn reject(&mut self, reason: &'static str) {
        if let Some(record) = &mut self.0 {
            record.outcome = "rejected";
            record.reason = reason;
        }
    }

    pub(super) fn reject_count(&mut self, reason: &'static str, count: usize) {
        self.reject(reason);
        if let Some(record) = &mut self.0 {
            record.failure = format!("-\t-\t-\t-\t-\tcount={count}");
        }
    }

    pub(super) fn reject_predicate<'tcx>(
        &mut self,
        tcx: TyCtxt<'tcx>,
        reason: &'static str,
        predicate: ty::Predicate<'tcx>,
        mask: ty::TypeFlags,
    ) {
        self.reject(reason);
        if let Some(record) = &mut self.0 {
            let offending = predicate.flags() & mask;
            record.failure = format!(
                "{}\t{}",
                predicate_fields(tcx, predicate, &mut record.limited),
                escape(&format!("offending={:#x}:{offending:?}", offending.bits()))
            );
        }
    }

    pub(super) fn reject_input<'tcx>(&mut self, tcx: TyCtxt<'tcx>, failure: InputFailure<'tcx>) {
        let Some(record) = &mut self.0 else { return };
        record.outcome = "rejected";
        let (reason, detail) = match failure {
            InputFailure::NodeBudget => ("input-node-budget", "budget=64".to_owned()),
            InputFailure::DynPredicateBudget { count, remaining } => {
                ("input-dyn-predicate-budget", format!("count={count};remaining={remaining}"))
            }
            InputFailure::AssociatedConst => {
                ("input-associated-const", "associated-const-binding".to_owned())
            }
            InputFailure::Type(ty) => {
                ("unsupported-input-type", shape(tcx, ty, 0, &mut record.limited))
            }
            InputFailure::Constant(constant) => {
                ("unsupported-input-const", constant_shape(tcx, constant, &mut record.limited))
            }
        };
        record.reason = reason;
        record.failure = format!("-\t-\t-\t-\t-\t{}", escape(&detail));
    }
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n")
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
    format!("L{}T{}C{}[{}]", kinds[0], kinds[1], kinds[2], shown.join(","))
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
            let items = types.iter().take(MAX_WIDTH).map(&mut child).collect::<Vec<_>>();
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

fn constant_shape<'tcx>(
    tcx: TyCtxt<'tcx>,
    constant: ty::Const<'tcx>,
    limited: &mut bool,
) -> String {
    match constant.kind() {
        ty::ConstKind::Value(value) => format!("Value({})", shape(tcx, value.ty, 0, limited)),
        ty::ConstKind::Param(_) => "Param".to_owned(),
        ty::ConstKind::Infer(_) => "Infer".to_owned(),
        ty::ConstKind::Bound(..) => "Bound".to_owned(),
        ty::ConstKind::Placeholder(_) => "Placeholder".to_owned(),
        ty::ConstKind::Error(_) => "Error".to_owned(),
        ty::ConstKind::Expr(_) => "Expr".to_owned(),
        ty::ConstKind::Alias(..) => "Alias".to_owned(),
    }
}

fn predicate_fields<'tcx>(
    tcx: TyCtxt<'tcx>,
    predicate: ty::Predicate<'tcx>,
    limited: &mut bool,
) -> String {
    let mut identity = "-".to_owned();
    let mut subject = None;
    let kind = match predicate.kind().skip_binder() {
        ty::PredicateKind::Clause(clause) => match clause {
            ty::ClauseKind::Trait(data) => {
                identity = nominal(tcx, data.def_id().into());
                subject = Some(data.self_ty());
                match data.polarity {
                    ty::PredicatePolarity::Positive => "TraitPositive",
                    ty::PredicatePolarity::Negative => "TraitNegative",
                }
            }
            ty::ClauseKind::HostEffect(data) => {
                identity = nominal(tcx, data.def_id().into());
                subject = Some(data.self_ty());
                "HostEffect"
            }
            ty::ClauseKind::Projection(data) => {
                // Use the stored associated-item identity; do not ask for its containing trait.
                let def_id = match data.projection_term.kind {
                    ty::AliasTermKind::ProjectionTy { def_id } => def_id.into(),
                    ty::AliasTermKind::InherentTy { def_id } => def_id.into(),
                    ty::AliasTermKind::OpaqueTy { def_id } => def_id.into(),
                    ty::AliasTermKind::FreeTy { def_id } => def_id.into(),
                    ty::AliasTermKind::AnonConst { def_id } => def_id.into(),
                    ty::AliasTermKind::ProjectionConst { def_id } => def_id.into(),
                    ty::AliasTermKind::FreeConst { def_id } => def_id.into(),
                    ty::AliasTermKind::InherentConst { def_id } => def_id.into(),
                };
                identity = nominal(tcx, def_id);
                subject = data.projection_term.args.types().next();
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
            identity = nominal(tcx, def_id.into());
            "DynCompatible"
        }
        ty::PredicateKind::Subtype(_) => "Subtype",
        ty::PredicateKind::Coerce(_) => "Coerce",
        ty::PredicateKind::ConstEquate(..) => "ConstEquate",
        ty::PredicateKind::Ambiguous => "Ambiguous",
        ty::PredicateKind::NormalizesTo(_) => "NormalizesTo",
    };
    let subject = subject.map_or_else(|| "-".to_owned(), |ty| shape(tcx, ty, 0, limited));
    let flags = predicate.flags();
    format!(
        "{}\t{}\t{}\t{}\t{}",
        kind,
        escape(&identity),
        escape(&subject),
        escape(&format!("{:#x}:{flags:?}", flags.bits())),
        predicate.kind().bound_vars().len()
    )
}
