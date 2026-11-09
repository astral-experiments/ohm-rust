use std::{cmp, ops};

use rustc_span::Span;

/// Tracks whether executing a node may exit normally (versus
/// return/break/panic, which "diverge", leaving dead code in their
/// wake). Tracked semi-automatically (through type variables marked
/// as diverging), with some manual adjustments for control-flow
/// primitives (approximating a CFG).
#[derive(Copy, Clone, Debug)]
pub(crate) enum Diverges {
    /// Potentially unknown, some cases converge.
    Maybe,

    /// Definitely known to diverge. The cause is used when diagnosing unreachable code.
    Always(UnreachableCause),

    /// Same as `Always` but with a reachability
    /// warning already emitted.
    WarnedAlways,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct UnreachableCause {
    span: Span,
    custom_note: Option<&'static str>,
}

impl UnreachableCause {
    fn cmp_for_selection(&self, other: &Self) -> cmp::Ordering {
        let a = self.span.data_untracked();
        let b = other.span.data_untracked();
        // With a shared parent, observing the winner below tracks the dependency for both.
        // If the parents differ, either position can change the winner independently.
        let order = if a.parent == b.parent { a.cmp(&b) } else { self.span.cmp(&other.span) };
        order.then_with(|| self.custom_note.cmp(&other.custom_note))
    }

    pub(super) fn diagnostic(self) -> (Span, Option<&'static str>) {
        let _ = self.span.data();
        (self.span, self.custom_note)
    }
}

impl ops::BitAnd for Diverges {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        match (self, other) {
            (Self::WarnedAlways, x) | (x, Self::WarnedAlways) => x,
            (Self::Always(a), Self::Always(b)) => {
                Self::Always(cmp::min_by(a, b, UnreachableCause::cmp_for_selection))
            }
            _ => Self::Maybe,
        }
    }
}

impl ops::BitOr for Diverges {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        match (self, other) {
            (Self::Maybe, x) | (x, Self::Maybe) => x,
            (Self::Always(a), Self::Always(b)) => {
                Self::Always(cmp::max_by(a, b, UnreachableCause::cmp_for_selection))
            }
            _ => Self::WarnedAlways,
        }
    }
}

impl ops::BitAndAssign for Diverges {
    fn bitand_assign(&mut self, other: Self) {
        *self = *self & other;
    }
}

impl ops::BitOrAssign for Diverges {
    fn bitor_assign(&mut self, other: Self) {
        *self = *self | other;
    }
}

impl Diverges {
    /// Creates a `Diverges::Always` with the provided `span` and the default note message.
    pub(super) fn always(span: Span) -> Diverges {
        Diverges::Always(UnreachableCause { span, custom_note: None })
    }

    pub(super) fn always_with_note(span: Span, note: &'static str) -> Diverges {
        Diverges::Always(UnreachableCause { span, custom_note: Some(note) })
    }

    pub(super) fn is_always(self) -> bool {
        matches!(self, Self::Always(_) | Self::WarnedAlways)
    }
}
