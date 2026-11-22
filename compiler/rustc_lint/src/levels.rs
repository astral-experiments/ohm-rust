use std::fmt::Debug;

use rustc_ast as ast;
use rustc_ast::attr::AttributeExt;
use rustc_ast_pretty::pprust;
use rustc_data_structures::fx::{FxHashSet, FxIndexMap};
use rustc_data_structures::unord::UnordSet;
use rustc_errors::{Diag, DiagCtxtHandle, Diagnostic, MultiSpan, msg};
use rustc_feature::{Features, GateIssue};
use rustc_hir as hir;
use rustc_hir::HirId;
use rustc_index::IndexVec;
use rustc_lint_defs::builtin::{
    self, FORBIDDEN_LINT_GROUPS, RENAMED_AND_REMOVED_LINTS, SINGLE_USE_LIFETIMES,
    UNFULFILLED_LINT_EXPECTATIONS, UNKNOWN_LINTS, UNUSED_ATTRIBUTES,
};
use rustc_lint_defs::{
    Level, Lint, LintExpectationId, LintId, StableLintExpectationId, UnstableLintExpectationId,
};
use rustc_middle::lint::{
    LevelSpec, LintExpectation, LintLevelSource, ShallowLintLevelMap, StableLevelSpec,
    UnstableLevelSpec, emit_lint_base, reveal_actual_level_spec,
};
use rustc_middle::query::Providers;
use rustc_middle::ty::{RegisteredTools, TyCtxt};
use rustc_session::Session;
use rustc_span::{AttrId, DUMMY_SP, Span, Symbol, sym};
use tracing::{debug, instrument};

use crate::builtin::MISSING_DOCS;
use crate::context::{CheckLintNameResult, LintStore};
use crate::diagnostics::{
    CheckNameUnknownTool, DeprecatedLintName, DeprecatedLintNameFromCommandLine,
    IgnoredUnlessCrateSpecified, MalformedAttribute, MalformedAttributeSub, OverruledAttribute,
    OverruledAttributeLint, OverruledAttributeSub, RemovedLint, RemovedLintFromCommandLine,
    RenamedLint, RenamedLintFromCommandLine, RenamedLintSuggestion, RequestedLevel, UnknownLint,
    UnknownLintFromCommandLine, UnknownLintSuggestion, UnknownToolInScopedLint, UnsupportedGroup,
};
use crate::late::unerased_lint_store;

/// Collection of lint levels for the whole crate.
/// This is used by AST-based lints, which do not
/// wait until we have built HIR to be emitted.
#[derive(Debug)]
struct LintLevelSets {
    /// Linked list of specifications.
    list: IndexVec<LintStackIndex, LintSet>,
}

rustc_index::newtype_index! {
    struct LintStackIndex {
        const COMMAND_LINE = 0;
    }
}

/// Specifications found at this position in the stack. This map only represents the lints
/// found for one set of attributes (like `shallow_lint_levels_on` does).
///
/// We store the level specifications as a linked list.
/// Each `LintSet` represents a set of attributes on the same AST node.
/// The `parent` forms a linked list that matches the AST tree.
/// This way, walking the linked list is equivalent to walking the AST bottom-up
/// to find the specifications for a given lint.
#[derive(Debug)]
struct LintSet {
    specs: FxIndexMap<LintId, UnstableLevelSpec>,
    parent: LintStackIndex,
}

impl LintLevelSets {
    fn new() -> Self {
        LintLevelSets { list: IndexVec::new() }
    }

    fn get_lint_level_spec(
        &self,
        lint: &'static Lint,
        idx: LintStackIndex,
        aux: Option<&FxIndexMap<LintId, UnstableLevelSpec>>,
        sess: &Session,
    ) -> UnstableLevelSpec {
        reveal_actual_level_spec(sess, LintId::of(lint), |id| {
            self.raw_lint_level_spec(id, idx, aux)
        })
    }

    fn raw_lint_level_spec(
        &self,
        id: LintId,
        mut idx: LintStackIndex,
        aux: Option<&FxIndexMap<LintId, UnstableLevelSpec>>,
    ) -> Option<UnstableLevelSpec> {
        if let Some(specs) = aux
            && let Some(level_spec) = specs.get(&id)
        {
            return Some(*level_spec);
        }

        loop {
            let LintSet { ref specs, parent } = self.list[idx];
            if let Some(level_spec) = specs.get(&id) {
                return Some(*level_spec);
            }
            if idx == COMMAND_LINE {
                return None;
            }
            idx = parent;
        }
    }
}

fn skippable_lints(tcx: TyCtxt<'_>, (): ()) -> UnordSet<LintId> {
    let store = unerased_lint_store(&tcx.sess);
    let root_map = tcx.shallow_lint_levels_on(hir::CRATE_OWNER_ID);

    let mut skippable: FxHashSet<LintId> = store
        .get_lints()
        .into_iter()
        .filter(|lint| {
            // Lints that show up in future-compat reports must always be run.
            let has_future_breakage =
                lint.future_incompatible.is_some_and(|fut| fut.report_in_deps);
            !has_future_breakage && !lint.eval_always
        })
        .filter(|lint| {
            let level_spec =
                root_map.lint_level_spec_at_node(tcx, LintId::of(lint), hir::CRATE_HIR_ID);
            // Only include lints that are allowed at crate root or by default.
            level_spec.is_allow()
                || (matches!(level_spec.src, LintLevelSource::Default)
                    && lint.default_level(tcx.sess.edition()) == Level::Allow)
        })
        .map(|lint| LintId::of(*lint))
        .collect();

    // Local attributes cannot raise a lint above the cap. The root already
    // accounts for forced warnings, and the filter above retains always-run
    // and future-incompatible lints. Check for expectations before skipping
    // local levels, since a suppressed lint can still fulfill an expectation.
    if tcx.sess.opts.lint_cap == Some(Level::Allow)
        && !tcx.sess.opts.unstable_opts.future_incompat_test
        && tcx.lint_expectations(()).is_empty()
    {
        return skippable.into();
    }

    for owner in tcx.hir_crate_items(()).owners() {
        let map = tcx.shallow_lint_levels_on(owner);

        // All lints that appear with a non-allow level must be run.
        for (_, specs) in map.specs.iter() {
            for (lint, level_spec) in specs.iter() {
                if !level_spec.is_allow() {
                    skippable.remove(lint);
                }
            }
        }
    }

    skippable.into()
}

#[instrument(level = "trace", skip(tcx), ret)]
fn shallow_lint_levels_on(tcx: TyCtxt<'_>, owner: hir::OwnerId) -> ShallowLintLevelMap {
    let attrs = tcx.hir_attr_map(owner);
    if attrs.map.is_empty() && owner != hir::CRATE_OWNER_ID {
        return ShallowLintLevelMap::default();
    }
    let store = unerased_lint_store(tcx.sess);

    let mut levels = LintLevelsBuilder {
        sess: tcx.sess,
        features: tcx.features(),
        provider: LintLevelQueryMap {
            tcx,
            cur: owner.into(),
            specs: ShallowLintLevelMap::default(),
            empty: FxIndexMap::default(),
        },
        lint_added_lints: false,
        store,
        registered_lint_tools: tcx.registered_lint_tools(()),
    };

    if owner == hir::CRATE_OWNER_ID {
        levels.add_command_line();
    }

    let entries = attrs.map.range(..);
    let mut add = |&(local_id, node_attrs): &(hir::ItemLocalId, &[hir::Attribute])| {
        let hir_id = HirId { owner, local_id };
        levels.provider.cur = hir_id;
        levels.add(node_attrs, hir_id == hir::CRATE_HIR_ID);
    };
    match entries {
        [] => {}
        [entry] => add(entry),
        _ => {
            for &index in attrs.lint_roots {
                add(&entries[index as usize]);
            }
        }
    }

    let specs = levels.provider.specs;

    #[cfg(debug_assertions)]
    for (_, v) in specs.specs.iter() {
        debug_assert!(!v.is_empty());
    }

    specs
}

pub struct TopDown {
    sets: LintLevelSets,
    cur: LintStackIndex,
}

pub trait LintLevelsProvider {
    type LintExpectationId: Copy + Debug + Into<LintExpectationId>;

    fn current_specs(&self) -> &FxIndexMap<LintId, LevelSpec<Self::LintExpectationId>>;

    fn insert(&mut self, id: LintId, level_spec: LevelSpec<Self::LintExpectationId>);

    fn get_lint_level_spec(
        &self,
        lint: &'static Lint,
        sess: &Session,
    ) -> LevelSpec<Self::LintExpectationId>;

    fn push_expectation(&mut self, id: Self::LintExpectationId, expectation: LintExpectation);

    fn mk_lint_expectation_id(
        &self,
        attr_id: AttrId,
        attr_index: usize,
        lint_index: u16,
    ) -> Self::LintExpectationId;
}

impl LintLevelsProvider for TopDown {
    type LintExpectationId = UnstableLintExpectationId;

    fn current_specs(&self) -> &FxIndexMap<LintId, UnstableLevelSpec> {
        &self.sets.list[self.cur].specs
    }

    fn insert(&mut self, id: LintId, level_spec: UnstableLevelSpec) {
        self.sets.list[self.cur].specs.insert(id, level_spec);
    }

    fn get_lint_level_spec(&self, lint: &'static Lint, sess: &Session) -> UnstableLevelSpec {
        self.sets.get_lint_level_spec(lint, self.cur, Some(self.current_specs()), sess)
    }

    fn push_expectation(&mut self, _: Self::LintExpectationId, _: LintExpectation) {}

    fn mk_lint_expectation_id(
        &self,
        attr_id: AttrId,
        _attr_index: usize,
        lint_index: u16,
    ) -> Self::LintExpectationId {
        UnstableLintExpectationId { attr_id, lint_index }
    }
}

struct LintLevelQueryMap<'tcx> {
    tcx: TyCtxt<'tcx>,
    cur: HirId,
    specs: ShallowLintLevelMap,
    /// Empty hash map to simplify code.
    empty: FxIndexMap<LintId, StableLevelSpec>,
}

impl LintLevelsProvider for LintLevelQueryMap<'_> {
    type LintExpectationId = StableLintExpectationId;

    fn current_specs(&self) -> &FxIndexMap<LintId, StableLevelSpec> {
        self.specs.specs.get(&self.cur.local_id).unwrap_or(&self.empty)
    }

    fn insert(&mut self, id: LintId, mut level_spec: StableLevelSpec) {
        // Moving an owner also moves its local lint settings. Keep their exact
        // positions observable when a diagnostic uses the source of an Allow.
        let owner = self.cur.owner;
        if self.tcx.dep_graph.is_fully_enabled()
            && owner != hir::CRATE_OWNER_ID
            && level_spec.level() == Level::Allow
            && let LintLevelSource::Node { span, .. } = &mut level_spec.src
        {
            let data = span.data_untracked();
            if data.parent.is_none()
                && !data.is_dummy()
                && self.tcx.source_span(owner.def_id).data_untracked().contains(data)
            {
                *span = span.with_parent(Some(owner.def_id));
            }
        }
        self.specs.specs.get_mut_or_insert_default(self.cur.local_id).insert(id, level_spec);
    }

    fn get_lint_level_spec(&self, lint: &'static Lint, _: &Session) -> StableLevelSpec {
        self.specs.lint_level_spec_at_node(self.tcx, LintId::of(lint), self.cur)
    }

    fn push_expectation(&mut self, id: Self::LintExpectationId, expectation: LintExpectation) {
        self.specs.expectations.push((id, expectation))
    }

    fn mk_lint_expectation_id(
        &self,
        _attr_id: AttrId,
        attr_index: usize,
        lint_index: u16,
    ) -> Self::LintExpectationId {
        let attr_index = attr_index.try_into().unwrap();
        StableLintExpectationId { hir_id: self.cur, attr_index, lint_index }
    }
}

pub struct LintLevelsBuilder<'s, P> {
    sess: &'s Session,
    features: &'s Features,
    provider: P,
    lint_added_lints: bool,
    store: &'s LintStore,
    registered_lint_tools: &'s RegisteredTools,
}

pub(crate) struct BuilderPush {
    prev: LintStackIndex,
}

impl<'s> LintLevelsBuilder<'s, TopDown> {
    pub(crate) fn new(
        sess: &'s Session,
        features: &'s Features,
        lint_added_lints: bool,
        store: &'s LintStore,
        registered_lint_tools: &'s RegisteredTools,
    ) -> Self {
        let mut builder = LintLevelsBuilder {
            sess,
            features,
            provider: TopDown { sets: LintLevelSets::new(), cur: COMMAND_LINE },
            lint_added_lints,
            store,
            registered_lint_tools,
        };
        builder.process_command_line();
        assert_eq!(builder.provider.sets.list.len(), 1);
        builder
    }

    pub fn crate_root(
        sess: &'s Session,
        features: &'s Features,
        lint_added_lints: bool,
        store: &'s LintStore,
        registered_lint_tools: &'s RegisteredTools,
        crate_attrs: &[ast::Attribute],
    ) -> Self {
        let mut builder = Self::new(sess, features, lint_added_lints, store, registered_lint_tools);
        builder.add(crate_attrs, true);
        builder
    }

    fn process_command_line(&mut self) {
        self.provider.cur = self
            .provider
            .sets
            .list
            .push(LintSet { specs: FxIndexMap::default(), parent: COMMAND_LINE });
        self.add_command_line();
    }

    /// Pushes a list of AST lint attributes onto this context.
    ///
    /// This function will return a `BuilderPush` object which should be passed
    /// to `pop` when this scope for the attributes provided is exited.
    ///
    /// This function will perform a number of tasks:
    ///
    /// * It'll validate all lint-related attributes in `attrs`
    /// * It'll mark all lint-related attributes as used
    /// * Lint levels will be updated based on the attributes provided
    /// * Lint attributes are validated, e.g., a `#[forbid]` can't be switched to
    ///   `#[allow]`
    ///
    /// Don't forget to call `pop`!
    pub(crate) fn push(&mut self, attrs: &[ast::Attribute], is_crate_node: bool) -> BuilderPush {
        let prev = self.provider.cur;
        if attrs.is_empty() {
            return BuilderPush { prev };
        }

        self.provider.cur =
            self.provider.sets.list.push(LintSet { specs: FxIndexMap::default(), parent: prev });

        self.add(attrs, is_crate_node);

        if self.provider.current_specs().is_empty() {
            self.provider.sets.list.pop();
            self.provider.cur = prev;
        }

        BuilderPush { prev }
    }

    /// Called after `push` when the scope of a set of attributes are exited.
    pub(crate) fn pop(&mut self, push: BuilderPush) {
        self.provider.cur = push.prev;
        std::mem::forget(push);
    }
}

#[cfg(debug_assertions)]
impl Drop for BuilderPush {
    fn drop(&mut self) {
        panic!("Found a `push` without a `pop`.");
    }
}

impl<'s, P: LintLevelsProvider> LintLevelsBuilder<'s, P>
where
    LevelSpec<P::LintExpectationId>: Into<LevelSpec>,
{
    pub(crate) fn sess(&self) -> &Session {
        self.sess
    }

    pub(crate) fn features(&self) -> &Features {
        self.features
    }

    fn add_command_line(&mut self) {
        for &(ref lint_name, level) in &self.sess.opts.lint_opts {
            // Checks the validity of lint names derived from the command line.
            let (tool_name, lint_name_only) = parse_lint_and_tool_name(lint_name);
            if lint_name_only == crate::WARNINGS.name_lower() && matches!(level, Level::ForceWarn) {
                self.sess
                    .dcx()
                    .emit_err(UnsupportedGroup { lint_group: crate::WARNINGS.name_lower() });
            }
            match self.store.check_lint_name(lint_name_only, tool_name, self.registered_lint_tools)
            {
                CheckLintNameResult::Renamed(ref replace) => {
                    let name = lint_name.as_str();
                    let suggestion = RenamedLintSuggestion::WithoutSpan { replace };
                    let requested_level = RequestedLevel { level, lint_name };
                    let lint =
                        RenamedLintFromCommandLine { name, replace, suggestion, requested_level };
                    self.emit_lint(RENAMED_AND_REMOVED_LINTS, lint);
                }
                CheckLintNameResult::Removed(ref reason) => {
                    let name = lint_name.as_str();
                    let requested_level = RequestedLevel { level, lint_name };
                    let lint = RemovedLintFromCommandLine { name, reason, requested_level };
                    self.emit_lint(RENAMED_AND_REMOVED_LINTS, lint);
                }
                CheckLintNameResult::NoLint(suggestion) => {
                    let name = lint_name.clone();
                    let suggestion = suggestion.map(|(replace, from_rustc)| {
                        UnknownLintSuggestion::WithoutSpan { replace, from_rustc }
                    });
                    let requested_level = RequestedLevel { level, lint_name };
                    let lint = UnknownLintFromCommandLine { name, suggestion, requested_level };
                    self.emit_lint(UNKNOWN_LINTS, lint);
                }
                CheckLintNameResult::Tool(_, Some(ref replace)) => {
                    let name = lint_name.clone();
                    let requested_level = RequestedLevel { level, lint_name };
                    let lint = DeprecatedLintNameFromCommandLine { name, replace, requested_level };
                    self.emit_lint(RENAMED_AND_REMOVED_LINTS, lint);
                }
                CheckLintNameResult::NoTool => {
                    self.sess.dcx().emit_err(CheckNameUnknownTool {
                        tool_name: tool_name.unwrap(),
                        sub: RequestedLevel { level, lint_name },
                    });
                }
                _ => {}
            };

            let lint_flag_val = Symbol::intern(lint_name);

            let Some(ids) = self.store.find_lints(lint_name) else {
                // errors already handled above
                continue;
            };
            for &id in ids {
                // ForceWarn and Forbid cannot be overridden
                if let Some(level_spec) = self.provider.current_specs().get(&id)
                    && matches!(level_spec.level(), Level::ForceWarn | Level::Forbid)
                {
                    continue;
                }

                if self.check_gated_lint(id, DUMMY_SP, true) {
                    let src = LintLevelSource::CommandLine(lint_flag_val, level);
                    self.provider.insert(id, LevelSpec::new(level, None, src));
                }
            }
        }
    }

    /// Attempts to insert the `id` to `LevelSpec` map entry. If unsuccessful
    /// (e.g. if a forbid was already inserted on the same scope), then emits a
    /// diagnostic with no change to `specs`.
    fn insert_spec(&mut self, id: LintId, level_spec: LevelSpec<P::LintExpectationId>) {
        let level = level_spec.level();
        let lint_id = level_spec.lint_id();
        let src = level_spec.src;

        let old_level_spec = self.provider.get_lint_level_spec(id.lint, self.sess);
        let old_level = old_level_spec.level();
        let old_src = old_level_spec.src;

        // Setting to a non-forbid level is an error if the lint previously had
        // a forbid level. Note that this is not necessarily true even with a
        // `#[forbid(..)]` attribute present, as that is overridden by `--cap-lints`.
        //
        // This means that this only errors if we're truly lowering the lint
        // level from forbid.
        if self.lint_added_lints && level == Level::Deny && old_level == Level::Forbid {
            // Having a deny inside a forbid is fine and is ignored, so we skip this check.
            return;
        } else if self.lint_added_lints && level != Level::Forbid && old_level == Level::Forbid {
            // Backwards compatibility check:
            //
            // We used to not consider `forbid(lint_group)`
            // as preventing `allow(lint)` for some lint `lint` in
            // `lint_group`. For now, issue a future-compatibility
            // warning for this case.
            let fcw_warning = match old_src {
                LintLevelSource::Default => false,
                LintLevelSource::Node { name, .. } => self.store.is_lint_group(name),
                LintLevelSource::CommandLine(symbol, _) => self.store.is_lint_group(symbol),
            };
            debug!(
                "fcw_warning={:?}, specs.get(&id) = {:?}, old_src={:?}, id_name={:?}",
                fcw_warning,
                self.provider.current_specs(),
                old_src,
                id.lint.name_lower(),
            );
            let sub = match old_src {
                LintLevelSource::Default => {
                    OverruledAttributeSub::DefaultSource { id: id.to_string() }
                }
                LintLevelSource::Node { span, reason, .. } => {
                    OverruledAttributeSub::NodeSource { span, reason }
                }
                LintLevelSource::CommandLine(name, _) => {
                    OverruledAttributeSub::CommandLineSource { id: name }
                }
            };
            if !fcw_warning {
                self.sess.dcx().emit_err(OverruledAttribute {
                    span: src.span(),
                    overruled: src.span(),
                    lint_level: level.as_str(),
                    lint_source: src.name(),
                    sub,
                });
            } else {
                self.emit_span_lint(
                    FORBIDDEN_LINT_GROUPS,
                    src.span().into(),
                    OverruledAttributeLint {
                        overruled: src.span(),
                        lint_level: level.as_str(),
                        lint_source: src.name(),
                        sub,
                    },
                );
            }

            // Retain the forbid lint level, unless we are
            // issuing a FCW. In the FCW case, we want to
            // respect the new setting.
            if !fcw_warning {
                return;
            }
        }

        // The lint `unfulfilled_lint_expectations` can't be expected, as it would suppress itself.
        // Handling expectations of this lint would add additional complexity with little to no
        // benefit. The expect level for this lint will therefore be ignored.
        if let Level::Expect = level
            && id == LintId::of(UNFULFILLED_LINT_EXPECTATIONS)
        {
            return;
        }

        match (old_level, level) {
            // If the new level is an expectation store it in `ForceWarn`
            (Level::ForceWarn, Level::Expect) => {
                self.provider.insert(id, LevelSpec::new(Level::ForceWarn, lint_id, old_src))
            }
            // Keep `ForceWarn` level but drop the expectation
            (Level::ForceWarn, _) => {
                self.provider.insert(id, LevelSpec::new(Level::ForceWarn, None, old_src))
            }
            // Set the lint level as normal
            _ => self.provider.insert(id, LevelSpec::new(level, lint_id, src)),
        };
    }

    fn add(&mut self, attrs: &[impl AttributeExt], is_crate_node: bool) {
        let sess = self.sess;
        for (attr_index, attr) in attrs.iter().enumerate() {
            if attr.is_automatically_derived_attr() {
                self.provider.insert(
                    LintId::of(SINGLE_USE_LIFETIMES),
                    LevelSpec::new(Level::Allow, None, LintLevelSource::Default),
                );
                continue;
            }

            // `#[doc(hidden)]` disables missing_docs check.
            if attr.is_doc_hidden() {
                self.provider.insert(
                    LintId::of(MISSING_DOCS),
                    LevelSpec::new(Level::Allow, None, LintLevelSource::Default),
                );
                continue;
            }

            let level = match Level::from_opt_symbol(attr.name()) {
                None => continue,
                Some(level) => level,
            };

            let Some(mut metas) = attr.meta_item_list() else { continue };

            // Check whether `metas` is empty, and get its last element.
            let Some(tail_li) = metas.last() else {
                // This emits the unused_attributes lint for `#[level()]`
                continue;
            };

            // Before processing the lint names, look for a reason (RFC 2383)
            // at the end.
            let mut reason = None;
            if let Some(item) = tail_li.meta_item() {
                match item.kind {
                    ast::MetaItemKind::Word => {} // actual lint names handled later
                    ast::MetaItemKind::NameValue(ref name_value) => {
                        if item.path == sym::reason {
                            if let ast::LitKind::Str(rationale, _) = name_value.kind {
                                reason = Some(rationale);
                            } else {
                                sess.dcx().emit_err(MalformedAttribute {
                                    span: name_value.span,
                                    sub: MalformedAttributeSub::ReasonMustBeStringLiteral(
                                        name_value.span,
                                    ),
                                });
                            }
                            // found reason, reslice meta list to exclude it
                            metas.pop().unwrap();
                        } else {
                            sess.dcx().emit_err(MalformedAttribute {
                                span: item.span,
                                sub: MalformedAttributeSub::BadAttributeArgument(item.span),
                            });
                        }
                    }
                    ast::MetaItemKind::List(_) => {
                        sess.dcx().emit_err(MalformedAttribute {
                            span: item.span,
                            sub: MalformedAttributeSub::BadAttributeArgument(item.span),
                        });
                    }
                }
            }

            for (lint_index, li) in metas.iter_mut().enumerate() {
                // `Expect` is the only lint level with a `LintExpectationId` that can be created
                // from an attribute.
                let lint_id = (level == Level::Expect).then(|| {
                    self.provider.mk_lint_expectation_id(attr.id(), attr_index, lint_index as u16)
                });

                let sp = li.span();
                let meta_item = match li {
                    ast::MetaItemInner::MetaItem(meta_item) if meta_item.is_word() => meta_item,
                    _ => {
                        let sub = if let Some(item) = li.meta_item()
                            && let ast::MetaItemKind::NameValue(_) = item.kind
                            && item.path == sym::reason
                        {
                            MalformedAttributeSub::ReasonMustComeLast(sp)
                        } else {
                            MalformedAttributeSub::BadAttributeArgument(sp)
                        };

                        sess.dcx().emit_err(MalformedAttribute { span: sp, sub });
                        continue;
                    }
                };
                let tool_ident = if meta_item.path.segments.len() > 1 {
                    Some(meta_item.path.segments.remove(0).ident)
                } else {
                    None
                };
                let tool_name = tool_ident.map(|ident| ident.name);
                let name = pprust::path_to_string(&meta_item.path);
                let lint_result =
                    self.store.check_lint_name(&name, tool_name, self.registered_lint_tools);

                let (ids, name) = match lint_result {
                    CheckLintNameResult::Ok(ids) => {
                        let name =
                            meta_item.path.segments.last().expect("empty lint name").ident.name;
                        (ids, name)
                    }

                    CheckLintNameResult::Tool(ids, new_lint_name) => {
                        let name = match new_lint_name {
                            None => {
                                let complete_name =
                                    &format!("{}::{}", tool_ident.unwrap().name, name);
                                Symbol::intern(complete_name)
                            }
                            Some(new_lint_name) => {
                                self.emit_span_lint(
                                    builtin::RENAMED_AND_REMOVED_LINTS,
                                    sp.into(),
                                    DeprecatedLintName {
                                        name,
                                        suggestion: sp,
                                        replace: &new_lint_name,
                                    },
                                );
                                Symbol::intern(&new_lint_name)
                            }
                        };
                        (ids, name)
                    }

                    CheckLintNameResult::MissingTool => {
                        // If `MissingTool` is returned, then either the lint does not
                        // exist in the tool or the code was not compiled with the tool and
                        // therefore the lint was never added to the `LintStore`. To detect
                        // this is the responsibility of the lint tool.
                        continue;
                    }

                    CheckLintNameResult::NoTool => {
                        sess.dcx().emit_err(UnknownToolInScopedLint {
                            span: tool_ident.map(|ident| ident.span),
                            tool_name: tool_name.unwrap(),
                            lint_name: pprust::path_to_string(&meta_item.path),
                            is_nightly_build: sess.is_nightly_build(),
                        });
                        continue;
                    }

                    CheckLintNameResult::Renamed(ref replace) => {
                        if self.lint_added_lints {
                            let suggestion =
                                RenamedLintSuggestion::WithSpan { suggestion: sp, replace };
                            let name =
                                tool_ident.map(|tool| format!("{tool}::{name}")).unwrap_or(name);
                            self.emit_span_lint(
                                RENAMED_AND_REMOVED_LINTS,
                                sp.into(),
                                RenamedLint { name: name.as_str(), replace, suggestion },
                            );
                        }

                        // If this lint was renamed, apply the new lint instead of ignoring the
                        // attribute. Ignore any errors or warnings that happen because the new
                        // name is inaccurate.
                        // NOTE: `new_name` already includes the tool name, so we don't
                        // have to add it again.
                        let CheckLintNameResult::Ok(ids) =
                            self.store.check_lint_name(replace, None, self.registered_lint_tools)
                        else {
                            panic!("renamed lint does not exist: {replace}");
                        };

                        (ids, Symbol::intern(&replace))
                    }

                    CheckLintNameResult::Removed(ref reason) => {
                        if self.lint_added_lints {
                            let name =
                                tool_ident.map(|tool| format!("{tool}::{name}")).unwrap_or(name);
                            self.emit_span_lint(
                                RENAMED_AND_REMOVED_LINTS,
                                sp.into(),
                                RemovedLint { name: name.as_str(), reason },
                            );
                        }
                        continue;
                    }

                    CheckLintNameResult::NoLint(suggestion) => {
                        if self.lint_added_lints {
                            let name =
                                tool_ident.map(|tool| format!("{tool}::{name}")).unwrap_or(name);
                            let suggestion = suggestion.map(|(replace, from_rustc)| {
                                UnknownLintSuggestion::WithSpan {
                                    suggestion: sp,
                                    replace,
                                    from_rustc,
                                }
                            });
                            self.emit_span_lint(
                                UNKNOWN_LINTS,
                                sp.into(),
                                UnknownLint { name, suggestion },
                            );
                        }
                        continue;
                    }
                };

                let src = LintLevelSource::Node { name, span: sp, reason };
                for &id in ids {
                    if self.check_gated_lint(id, sp, false) {
                        self.insert_spec(id, LevelSpec::new(level, lint_id, src));
                    }
                }

                // This checks for instances where the user writes
                // `#[expect(unfulfilled_lint_expectations)]` in that case we want to avoid
                // overriding the lint level but instead add an expectation that can't be
                // fulfilled. The lint message will include an explanation, that the
                // `unfulfilled_lint_expectations` lint can't be expected.
                if let (Level::Expect, Some(expect_id)) = (level, lint_id) {
                    // The `unfulfilled_lint_expectations` lint is not part of any lint
                    // groups. Therefore. we only need to check the slice if it contains a
                    // single lint.
                    let is_unfulfilled_lint_expectations = match ids {
                        [lint] => *lint == LintId::of(UNFULFILLED_LINT_EXPECTATIONS),
                        _ => false,
                    };
                    self.provider.push_expectation(
                        expect_id,
                        LintExpectation::new(
                            reason,
                            sp,
                            is_unfulfilled_lint_expectations,
                            tool_name,
                        ),
                    );
                }
            }
        }

        if self.lint_added_lints && !is_crate_node {
            for (id, level_spec) in self.provider.current_specs().iter() {
                if !id.lint.crate_level_only {
                    continue;
                }

                let LintLevelSource::Node { name: lint_attr_name, span: lint_attr_span, .. } =
                    level_spec.src
                else {
                    continue;
                };

                self.emit_span_lint(
                    UNUSED_ATTRIBUTES,
                    lint_attr_span.into(),
                    IgnoredUnlessCrateSpecified {
                        level: level_spec.level().as_str(),
                        name: lint_attr_name,
                    },
                );
                // don't set a separate error for every lint in the group
                break;
            }
        }
    }

    /// Checks if the lint is gated on a feature that is not enabled.
    ///
    /// Returns `true` if the lint's feature is enabled.
    #[track_caller]
    fn check_gated_lint(&self, lint_id: LintId, span: Span, lint_from_cli: bool) -> bool {
        let feature = if let Some(feature) = lint_id.lint.feature_gate
            && !self.features.enabled(feature)
            && !span.allows_unstable(feature)
        {
            // Lint is behind a feature that is not enabled; eventually return false.
            feature
        } else {
            // Lint is ungated or its feature is enabled; exit early.
            return true;
        };

        struct UnknownLint<'a> {
            sess: &'a Session,
            lint_id: LintId,
            feature: Symbol,
            lint_from_cli: bool,
        }

        impl<'a, 'b> Diagnostic<'a> for UnknownLint<'b> {
            fn into_diag(self, dcx: DiagCtxtHandle<'a>, level: rustc_errors::Level) -> Diag<'a> {
                let Self { sess, lint_id, feature, lint_from_cli } = self;
                let mut lint = Diag::new(dcx, level, msg!("unknown lint: `{$name}`"))
                    .with_arg("name", lint_id.lint.name_lower())
                    .with_note(msg!("the `{$name}` lint is unstable"));
                // `staged_api` is only intended for the standard library, so don't
                // suggest enabling it just to use this lint.
                if feature != sym::staged_api {
                    rustc_session::diagnostics::add_feature_diagnostics_for_issue(
                        &mut lint,
                        sess,
                        feature,
                        GateIssue::Language,
                        lint_from_cli,
                        None,
                    );
                }
                lint
            }
        }

        if self.lint_added_lints {
            let lint = builtin::UNKNOWN_LINTS;
            let level_spec = self.lint_level_spec(builtin::UNKNOWN_LINTS);
            emit_lint_base(
                self.sess,
                lint,
                level_spec,
                Some(span.into()),
                UnknownLint { sess: &self.sess, lint_id, feature, lint_from_cli },
            );
        }

        false
    }

    /// Find the lint level for a lint.
    pub fn lint_level_spec(&self, lint: &'static Lint) -> LevelSpec<P::LintExpectationId> {
        self.provider.get_lint_level_spec(lint, self.sess)
    }

    /// Used to emit a lint-related diagnostic based on the current state of
    /// this lint context.
    #[track_caller]
    pub(crate) fn opt_span_lint(
        &self,
        lint: &'static Lint,
        span: Option<MultiSpan>,
        decorator: impl for<'a> Diagnostic<'a>,
    ) {
        let level_spec = self.lint_level_spec(lint);
        emit_lint_base(self.sess, lint, level_spec, span, decorator)
    }

    #[track_caller]
    pub fn emit_span_lint(
        &self,
        lint: &'static Lint,
        span: MultiSpan,
        decorator: impl for<'a> Diagnostic<'a>,
    ) {
        let level_spec = self.lint_level_spec(lint);
        emit_lint_base(self.sess, lint, level_spec, Some(span), decorator);
    }

    #[track_caller]
    pub fn emit_lint(&self, lint: &'static Lint, decorator: impl for<'a> Diagnostic<'a>) {
        let level_spec = self.lint_level_spec(lint);
        emit_lint_base(self.sess, lint, level_spec, None, decorator);
    }
}

pub(crate) fn provide(providers: &mut Providers) {
    *providers = Providers { shallow_lint_levels_on, skippable_lints, ..*providers };
}

pub(crate) fn parse_lint_and_tool_name(lint_name: &str) -> (Option<Symbol>, &str) {
    match lint_name.split_once("::") {
        Some((tool_name, lint_name)) => {
            let tool_name = Symbol::intern(tool_name);

            (Some(tool_name), lint_name)
        }
        None => (None, lint_name),
    }
}
