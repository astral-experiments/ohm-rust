use std::fmt;

use thin_vec::thin_vec;

use super::*;

impl<'a> super::ForestObligation for &'a str {
    type CacheKey = &'a str;

    fn as_cache_key(&self) -> Self::CacheKey {
        self
    }
}

struct ClosureObligationProcessor<OF, BF, O, E> {
    process_obligation: OF,
    _process_backedge: BF,
    marker: PhantomData<(O, E)>,
}

struct TestOutcome<O, E> {
    pub completed: Vec<O>,
    pub errors: Vec<Error<O, E>>,
}

impl<O, E> OutcomeTrait for TestOutcome<O, E>
where
    O: Clone,
{
    type Error = Error<O, E>;
    type Obligation = O;

    fn new() -> Self {
        Self { errors: vec![], completed: vec![] }
    }

    fn record_completed(&mut self, outcome: &Self::Obligation) {
        self.completed.push(outcome.clone())
    }

    fn record_error(&mut self, error: Self::Error) {
        self.errors.push(error)
    }
}

#[allow(non_snake_case)]
fn C<OF, BF, O>(of: OF, bf: BF) -> ClosureObligationProcessor<OF, BF, O, &'static str>
where
    OF: FnMut(&mut O) -> ProcessResult<O, &'static str>,
    BF: FnMut(&[O]),
{
    ClosureObligationProcessor {
        process_obligation: of,
        _process_backedge: bf,
        marker: PhantomData,
    }
}

impl<OF, BF, O, E> ObligationProcessor for ClosureObligationProcessor<OF, BF, O, E>
where
    O: super::ForestObligation + fmt::Debug,
    E: fmt::Debug,
    OF: FnMut(&mut O) -> ProcessResult<O, E>,
    BF: FnMut(&[O]),
{
    type Obligation = O;
    type Child = O;
    type Error = E;
    type OUT = TestOutcome<O, E>;

    fn needs_process_obligation(&self, _obligation: &Self::Obligation) -> bool {
        true
    }

    fn process_obligation(
        &mut self,
        obligation: &mut Self::Obligation,
    ) -> ProcessResult<Self::Obligation, Self::Error> {
        (self.process_obligation)(obligation)
    }

    fn prepare_child(&self, _parent: &O, child: O) -> Option<O> {
        Some(child)
    }

    fn process_backedge<'c, I>(
        &mut self,
        _cycle: I,
        _marker: PhantomData<&'c Self::Obligation>,
    ) -> Result<(), Self::Error>
    where
        I: Clone + Iterator<Item = &'c Self::Obligation>,
    {
        Ok(())
    }
}

#[test]
fn prepare_children_preserves_parent_and_order() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Obligation(&'static str, usize);

    impl ForestObligation for Obligation {
        type CacheKey = &'static str;

        fn as_cache_key(&self) -> Self::CacheKey {
            self.0
        }
    }

    struct Processor {
        processed: Vec<Obligation>,
    }

    impl ObligationProcessor for Processor {
        type Obligation = Obligation;
        type Child = &'static str;
        type Error = &'static str;
        type OUT = TestOutcome<Obligation, Self::Error>;

        fn needs_process_obligation(&self, _obligation: &Obligation) -> bool {
            true
        }

        fn process_obligation(
            &mut self,
            obligation: &mut Obligation,
        ) -> ProcessResult<Self::Child, Self::Error> {
            self.processed.push(obligation.clone());
            match obligation.0 {
                "root" => ProcessResult::Changed(thin_vec!["proven", "left", "right"]),
                "proven_parent" => ProcessResult::Changed(thin_vec!["proven"]),
                "left" => ProcessResult::Changed(thin_vec!["leaf"]),
                "right" => ProcessResult::Changed(thin_vec![]),
                "leaf" => ProcessResult::Error("leaf failed"),
                _ => panic!("unexpected obligation: {obligation:?}"),
            }
        }

        fn prepare_child(&self, parent: &Obligation, child: Self::Child) -> Option<Obligation> {
            (child != "proven").then(|| Obligation(child, parent.1 + 1))
        }

        fn process_backedge<'c, I>(
            &mut self,
            _cycle: I,
            _marker: PhantomData<&'c Obligation>,
        ) -> Result<(), Self::Error>
        where
            I: Clone + Iterator<Item = &'c Obligation>,
        {
            panic!("unexpected cycle")
        }
    }

    let mut forest = ObligationForest::new();
    forest.register_obligation(Obligation("root", 0));
    forest.register_obligation(Obligation("proven_parent", 0));
    let mut processor = Processor { processed: vec![] };
    let outcome = forest.process_obligations(&mut processor);
    assert_eq!(
        processor.processed,
        vec![
            Obligation("root", 0),
            Obligation("proven_parent", 0),
            Obligation("left", 1),
            Obligation("right", 1),
            Obligation("leaf", 2),
        ]
    );
    assert_eq!(outcome.errors.len(), 1);
    assert_eq!(outcome.errors[0].error, "leaf failed");
    assert_eq!(
        outcome.errors[0].backtrace,
        vec![Obligation("leaf", 2), Obligation("left", 1), Obligation("root", 0)]
    );
    assert_eq!(outcome.completed, vec![Obligation("proven_parent", 0), Obligation("right", 1)]);
    assert_eq!(forest.len(), 0);
}

#[test]
fn push_pop() {
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");
    forest.register_obligation("B");
    forest.register_obligation("C");

    // first round, B errors out, A has subtasks, and C completes, creating this:
    //      A |-> A.1
    //        |-> A.2
    //        |-> A.3
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Changed(thin_vec!["A.1", "A.2", "A.3"]),
            "B" => ProcessResult::Error("B is for broken"),
            "C" => ProcessResult::Changed(thin_vec![]),
            "A.1" | "A.2" | "A.3" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok, vec!["C"]);
    assert_eq!(err, vec![Error { error: "B is for broken", backtrace: vec!["B"] }]);

    // second round: two delays, one success, creating an uneven set of subtasks:
    //      A |-> A.1
    //        |-> A.2
    //        |-> A.3 |-> A.3.i
    //      D |-> D.1
    //        |-> D.2
    forest.register_obligation("D");
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.1" => ProcessResult::Unchanged,
            "A.2" => ProcessResult::Unchanged,
            "A.3" => ProcessResult::Changed(thin_vec!["A.3.i"]),
            "D" => ProcessResult::Changed(thin_vec!["D.1", "D.2"]),
            "A.3.i" | "D.1" | "D.2" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok, Vec::<&'static str>::new());
    assert_eq!(err, Vec::new());

    // third round: ok in A.1 but trigger an error in A.2. Check that it
    // propagates to A, but not D.1 or D.2.
    //      D |-> D.1 |-> D.1.i
    //        |-> D.2 |-> D.2.i
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.1" => ProcessResult::Changed(thin_vec![]),
            "A.2" => ProcessResult::Error("A is for apple"),
            "A.3.i" => ProcessResult::Changed(thin_vec![]),
            "D.1" => ProcessResult::Changed(thin_vec!["D.1.i"]),
            "D.2" => ProcessResult::Changed(thin_vec!["D.2.i"]),
            "D.1.i" | "D.2.i" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["A.1", "A.3", "A.3.i"]);
    assert_eq!(err, vec![Error { error: "A is for apple", backtrace: vec!["A.2", "A"] }]);

    // fourth round: error in D.1.i
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D.1.i" => ProcessResult::Error("D is for dumb"),
            "D.2.i" => ProcessResult::Changed(thin_vec![]),
            _ => panic!("unexpected obligation {:?}", obligation),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["D.2", "D.2.i"]);
    assert_eq!(err, vec![Error { error: "D is for dumb", backtrace: vec!["D.1.i", "D.1", "D"] }]);
}

// Test that if a tree with grandchildren succeeds, everything is
// reported as expected:
// A
//   A.1
//   A.2
//      A.2.i
//      A.2.ii
//   A.3
#[test]
fn success_in_grandchildren() {
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Changed(thin_vec!["A.1", "A.2", "A.3"]),
            "A.1" => ProcessResult::Changed(thin_vec![]),
            "A.2" => ProcessResult::Changed(thin_vec!["A.2.i", "A.2.ii"]),
            "A.3" => ProcessResult::Changed(thin_vec![]),
            "A.2.i" | "A.2.ii" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["A.1", "A.3"]);
    assert!(err.is_empty());

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.2.i" => ProcessResult::Unchanged,
            "A.2.ii" => ProcessResult::Changed(thin_vec![]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok, vec!["A.2.ii"]);
    assert!(err.is_empty());

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.2.i" => ProcessResult::Changed(thin_vec!["A.2.i.a"]),
            "A.2.i.a" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert!(ok.is_empty());
    assert!(err.is_empty());

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.2.i.a" => ProcessResult::Changed(thin_vec![]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["A", "A.2", "A.2.i", "A.2.i.a"]);
    assert!(err.is_empty());

    let TestOutcome { completed: ok, errors: err, .. } =
        forest.process_obligations(&mut C(|_| unreachable!(), |_| {}));

    assert!(ok.is_empty());
    assert!(err.is_empty());
}

#[test]
fn to_errors_no_throw() {
    // check that converting multiple children with common parent (A)
    // yields to correct errors (and does not panic, in particular).
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Changed(thin_vec!["A.1", "A.2", "A.3"]),
            "A.1" | "A.2" | "A.3" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);
    let errors = forest.to_errors(());
    assert_eq!(errors[0].backtrace, vec!["A.1", "A"]);
    assert_eq!(errors[1].backtrace, vec!["A.2", "A"]);
    assert_eq!(errors[2].backtrace, vec!["A.3", "A"]);
    assert_eq!(errors.len(), 3);
}

#[test]
fn diamond() {
    // check that diamond dependencies are handled correctly
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Changed(thin_vec!["A.1", "A.2"]),
            "A.1" | "A.2" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A.1" => ProcessResult::Changed(thin_vec!["D"]),
            "A.2" => ProcessResult::Changed(thin_vec!["D"]),
            "D" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);

    let mut d_count = 0;
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D" => {
                d_count += 1;
                ProcessResult::Changed(thin_vec![])
            }
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(d_count, 1);
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["A", "A.1", "A.2", "D"]);
    assert_eq!(err.len(), 0);

    let errors = forest.to_errors(());
    assert_eq!(errors.len(), 0);

    forest.register_obligation("A'");
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A'" => ProcessResult::Changed(thin_vec!["A'.1", "A'.2"]),
            "A'.1" | "A'.2" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A'.1" => ProcessResult::Changed(thin_vec!["D'", "A'"]),
            "A'.2" => ProcessResult::Changed(thin_vec!["D'"]),
            "D'" | "A'" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);

    let mut d_count = 0;
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D'" => {
                d_count += 1;
                ProcessResult::Error("operation failed")
            }
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(d_count, 1);
    assert_eq!(ok.len(), 0);
    assert_eq!(
        err,
        vec![super::Error { error: "operation failed", backtrace: vec!["D'", "A'.1", "A'"] }]
    );

    let errors = forest.to_errors(());
    assert_eq!(errors.len(), 0);
}

#[test]
fn shared_dependency_after_compaction() {
    // Share one dependency among four roots, then rewrite its dependents when
    // an earlier node, and optionally its original parent, are removed.
    for remove_parent in [false, true] {
        let mut forest = ObligationForest::new();
        for obligation in ["done", "A", "B", "C", "D"] {
            forest.register_obligation(obligation);
        }

        let TestOutcome { completed, errors } = forest.process_obligations(&mut C(
            |obligation| match *obligation {
                "done" => ProcessResult::Changed(thin_vec![]),
                "A" if remove_parent => ProcessResult::Changed(thin_vec!["shared", "parent_error"]),
                "A" | "B" | "C" | "D" => ProcessResult::Changed(thin_vec!["shared"]),
                "shared" | "parent_error" => ProcessResult::Unchanged,
                _ => unreachable!(),
            },
            |_| {},
        ));
        assert_eq!(completed, vec!["done"]);
        assert!(errors.is_empty());

        if remove_parent {
            let TestOutcome { completed, errors } = forest.process_obligations(&mut C(
                |obligation| match *obligation {
                    "shared" => ProcessResult::Unchanged,
                    "parent_error" => ProcessResult::Error("parent failed"),
                    _ => unreachable!(),
                },
                |_| {},
            ));
            assert!(completed.is_empty());
            assert_eq!(
                errors,
                vec![Error { error: "parent failed", backtrace: vec!["parent_error", "A"] }]
            );
        }

        let TestOutcome { completed, errors } = forest.process_obligations(&mut C(
            |obligation| {
                assert_eq!(*obligation, "shared");
                ProcessResult::Error("shared failed")
            },
            |_| {},
        ));
        let backtrace = if remove_parent { vec!["shared"] } else { vec!["shared", "A"] };
        assert!(completed.is_empty());
        assert_eq!(errors, vec![Error { error: "shared failed", backtrace }]);
        assert_eq!(forest.len(), 0);
    }
}

#[test]
fn done_dependency() {
    // check that the local cache works
    let mut forest = ObligationForest::new();
    forest.register_obligation("A: Sized");
    forest.register_obligation("B: Sized");
    forest.register_obligation("C: Sized");

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A: Sized" | "B: Sized" | "C: Sized" => ProcessResult::Changed(thin_vec![]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["A: Sized", "B: Sized", "C: Sized"]);
    assert_eq!(err.len(), 0);

    forest.register_obligation("(A,B,C): Sized");
    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "(A,B,C): Sized" => {
                ProcessResult::Changed(thin_vec!["A: Sized", "B: Sized", "C: Sized"])
            }
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok, vec!["(A,B,C): Sized"]);
    assert_eq!(err.len(), 0);
}

#[test]
fn orphan() {
    // check that orphaned nodes are handled correctly
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");
    forest.register_obligation("B");
    forest.register_obligation("C1");
    forest.register_obligation("C2");

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Changed(thin_vec!["D", "E"]),
            "B" => ProcessResult::Unchanged,
            "C1" => ProcessResult::Changed(thin_vec![]),
            "C2" => ProcessResult::Changed(thin_vec![]),
            "D" | "E" => ProcessResult::Unchanged,
            _ => unreachable!(),
        },
        |_| {},
    ));
    let mut ok = ok;
    ok.sort();
    assert_eq!(ok, vec!["C1", "C2"]);
    assert_eq!(err.len(), 0);

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D" | "E" => ProcessResult::Unchanged,
            "B" => ProcessResult::Changed(thin_vec!["D"]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err.len(), 0);

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D" => ProcessResult::Unchanged,
            "E" => ProcessResult::Error("E is for error"),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err, vec![super::Error { error: "E is for error", backtrace: vec!["E", "A"] }]);

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "D" => ProcessResult::Error("D is dead"),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err, vec![super::Error { error: "D is dead", backtrace: vec!["D"] }]);

    let errors = forest.to_errors(());
    assert_eq!(errors.len(), 0);
}

#[test]
fn simultaneous_register_and_error() {
    // check that registering a failed obligation works correctly
    let mut forest = ObligationForest::new();
    forest.register_obligation("A");
    forest.register_obligation("B");

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Error("An error"),
            "B" => ProcessResult::Changed(thin_vec!["A"]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err, vec![super::Error { error: "An error", backtrace: vec!["A"] }]);

    let mut forest = ObligationForest::new();
    forest.register_obligation("B");
    forest.register_obligation("A");

    let TestOutcome { completed: ok, errors: err, .. } = forest.process_obligations(&mut C(
        |obligation| match *obligation {
            "A" => ProcessResult::Error("An error"),
            "B" => ProcessResult::Changed(thin_vec!["A"]),
            _ => unreachable!(),
        },
        |_| {},
    ));
    assert_eq!(ok.len(), 0);
    assert_eq!(err, vec![super::Error { error: "An error", backtrace: vec!["A"] }]);
}
