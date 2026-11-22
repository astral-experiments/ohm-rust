//! Detecting usage of the `#[debugger_visualizer]` attribute.

use rustc_ast::{ItemKind, ast};
use rustc_attr_ir::{Attribute, AttributeKind, DebugVisualizer};
use rustc_attr_parsing::AttributeParser;
use rustc_middle::middle::debugger_visualizer::DebuggerVisualizerFile;
use rustc_middle::query::{LocalCrate, Providers};
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_span::sym;

use crate::diagnostics::DebugVisualizerUnreadable;

impl DebuggerVisualizerCollector<'_, '_> {
    fn check_for_debugger_visualizer(&mut self, attrs: &[ast::Attribute]) {
        if let Some(Attribute::Parsed(AttributeKind::DebuggerVisualizer(visualizers))) =
            AttributeParser::parse_limited_sym(&self.sess, attrs, &[sym::debugger_visualizer])
        {
            for DebugVisualizer { span, visualizer_type, path } in visualizers {
                let file = match self.sess.resolve_path(path.as_str(), span) {
                    Ok(file) => file,
                    Err(err) => {
                        err.emit();
                        return;
                    }
                };

                match self.sess.source_map().load_binary_file(&file) {
                    Ok((source, _)) => {
                        self.visualizers.push(DebuggerVisualizerFile::new(
                            source,
                            visualizer_type,
                            file,
                        ));
                    }
                    Err(error) => {
                        self.sess.dcx().emit_err(DebugVisualizerUnreadable {
                            span,
                            file: &file,
                            error,
                        });
                    }
                }
            }
        }
    }
}

struct DebuggerVisualizerCollector<'a, 'tcx> {
    resolver: &'a rustc_middle::middle::resolve::ResolverAstLowering<'tcx>,
    sess: &'a Session,
    visualizers: Vec<DebuggerVisualizerFile>,
}

impl<'ast> rustc_ast::visit::Visitor<'ast> for DebuggerVisualizerCollector<'_, '_> {
    fn visit_item(&mut self, item: &'ast rustc_ast::Item) -> Self::Result {
        if let ItemKind::Mod(..) = item.kind {
            self.check_for_debugger_visualizer(&item.attrs);
        }
        if self.resolver.owners[&item.id].may_have_nested_items {
            rustc_ast::visit::walk_item(self, item);
        }
    }

    fn visit_assoc_item(&mut self, item: &'ast ast::AssocItem, ctxt: rustc_ast::visit::AssocCtxt) {
        if self.resolver.owners[&item.id].may_have_nested_items {
            rustc_ast::visit::walk_assoc_item(self, item, ctxt);
        }
    }
    fn visit_crate(&mut self, krate: &'ast ast::Crate) -> Self::Result {
        self.check_for_debugger_visualizer(&krate.attrs);
        rustc_ast::visit::walk_crate(self, krate);
    }
}

/// Traverses and collects the debugger visualizers for a specific crate.
fn debugger_visualizers(tcx: TyCtxt<'_>, _: LocalCrate) -> Vec<DebuggerVisualizerFile> {
    let (resolver, krate) = tcx.resolver_for_lowering();
    let resolver = &*resolver.borrow();

    let mut visitor =
        DebuggerVisualizerCollector { resolver, sess: tcx.sess, visualizers: Vec::new() };
    rustc_ast::visit::Visitor::visit_crate(&mut visitor, &*krate.borrow());

    // We are collecting visualizers in AST-order, which is deterministic,
    // so we don't need to do any explicit sorting in order to get a
    // deterministic query result
    visitor.visualizers
}

pub(crate) fn provide(providers: &mut Providers) {
    providers.debugger_visualizers = debugger_visualizers;
}
