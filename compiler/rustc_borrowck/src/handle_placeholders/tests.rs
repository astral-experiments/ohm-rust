use rustc_data_structures::graph::vec_graph::VecGraph;

use super::*;

#[test]
fn no_placeholder_annotations_match_full_tracker() {
    // Exhaust all directed graphs on three nodes, including self edges and
    // disconnected graphs. Vary origins and universes independently so the
    // SCC representative need not be the least-universed reachable region.
    for free_regions in 0..8 {
        for universes in [[0, 0, 0], [0, 1, 2], [2, 0, 1], [1, 2, 0]] {
            let definitions = universes
                .into_iter()
                .enumerate()
                .map(|(index, universe)| RegionDefinition {
                    origin: if free_regions & (1 << index) != 0 {
                        NllRegionVariableOrigin::FreeRegion
                    } else {
                        NllRegionVariableOrigin::Existential { name: None }
                    },
                    universe: UniverseIndex::from_u32(universe),
                    external_name: None,
                })
                .collect();

            for edges in 0..512 {
                let edges = (0..9)
                    .filter(|edge| edges & (1 << edge) != 0)
                    .map(|edge| (RegionVid::from_u32(edge / 3), RegionVid::from_u32(edge % 3)))
                    .collect();
                let graph = VecGraph::<RegionVid>::new(3, edges);
                let mut full_annotations = SccAnnotations::<RegionTracker>::init(&definitions);
                let mut compact_annotations =
                    NoPlaceholderSccAnnotations(SccAnnotations::init(&definitions));
                let full = ConstraintSccs::new_with_annotation(&graph, &mut full_annotations);
                let compact = ConstraintSccs::new_with_annotation(&graph, &mut compact_annotations);

                assert_eq!(full.scc_indices(), compact.scc_indices());
                for scc in full.all_sccs() {
                    assert_eq!(full.successors(scc), compact.successors(scc));
                    assert_eq!(
                        full_annotations.scc_to_annotation[scc],
                        compact_annotations.0.scc_to_annotation[scc]
                    );
                }
            }
        }
    }
}
