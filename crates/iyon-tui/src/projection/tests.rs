use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::{StreamOffset, StreamRange};

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

fn complete<T>(stable: u64, sealed: bool) -> ProjectionBuilder<T> {
    ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(stable),
        StreamOffset::new(4),
        sealed,
    )
}

#[test]
fn construction_requires_exact_nonempty_coverage_and_allows_elision() {
    assert!(
        ProjectionBuilder::<u8>::new(
            StreamOffset::ZERO,
            StreamOffset::ZERO,
            StreamOffset::ZERO,
            false
        )
        .finish()
        .is_ok()
    );
    assert!(
        complete::<u8>(0, false)
            .emit(range(0, 0), 1)
            .finish()
            .is_err()
    );
    assert!(
        complete::<u8>(0, false)
            .emit(range(1, 4), 1)
            .finish()
            .is_err()
    );
    assert!(
        complete::<u8>(0, false)
            .elide(range(0, 3))
            .emit(range(2, 4), 1)
            .finish()
            .is_err()
    );
    assert!(
        complete::<u8>(0, false)
            .elide(range(0, 2))
            .emit(range(2, 4), 1)
            .finish()
            .is_ok()
    );
}

#[test]
fn stable_frontier_must_be_a_span_boundary() {
    assert_eq!(
        complete::<u8>(1, false)
            .emit(range(0, 2), 1)
            .elide(range(2, 4))
            .finish(),
        Err(ProjectionValidationError::StableFrontierInsideSpan)
    );
    assert!(
        complete::<u8>(2, false)
            .emit(range(0, 2), 1)
            .elide(range(2, 4))
            .finish()
            .is_ok()
    );
    assert!(
        complete::<u8>(4, true)
            .emit(range(0, 4), 1)
            .finish()
            .is_ok()
    );
}

#[test]
fn transitions_freeze_values_and_segmentation_but_allow_tail_replacement() {
    let previous = complete::<u8>(2, false)
        .emit(range(0, 2), 1)
        .elide(range(2, 4))
        .finish()
        .unwrap();
    let next = complete::<u8>(4, true)
        .emit(range(0, 2), 1)
        .emit_many(range(2, 4), [2, 3])
        .finish()
        .unwrap();
    validate_projection_transition(&previous, &next).unwrap();

    let changed = complete::<u8>(2, false)
        .emit(range(0, 2), 9)
        .elide(range(2, 4))
        .finish()
        .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &changed),
        Err(ProjectionTransitionError::StablePrefixChanged)
    );

    let resegmented = complete::<u8>(2, false).emit(range(0, 4), 1).finish();
    assert_eq!(
        resegmented,
        Err(ProjectionValidationError::StableFrontierInsideSpan)
    );
}

#[test]
fn compaction_may_remove_only_stable_prefix() {
    let previous = complete::<u8>(2, false)
        .emit(range(0, 2), 1)
        .elide(range(2, 4))
        .finish()
        .unwrap();
    let compacted = ProjectionBuilder::new(
        StreamOffset::new(2),
        StreamOffset::new(2),
        StreamOffset::new(4),
        false,
    )
    .elide(range(2, 4))
    .finish()
    .unwrap();
    validate_projection_transition(&previous, &compacted).unwrap();

    let invalid = ProjectionBuilder::new(
        StreamOffset::new(3),
        StreamOffset::new(3),
        StreamOffset::new(4),
        false,
    )
    .elide(range(3, 4))
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &invalid),
        Err(ProjectionTransitionError::SourceBaseBeyondPreviousStability)
    );
}

#[test]
fn relation_allows_lagging_and_sealed_input() {
    let input = ProjectionBuilder::<u8>::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        true,
    )
    .emit(range(0, 4), 1)
    .finish()
    .unwrap();
    let output = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(2),
        StreamOffset::new(2),
        false,
    )
    .emit(range(0, 2), 1)
    .finish()
    .unwrap();
    validate_projection_relation(&input, &output).unwrap();
    let final_output = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        true,
    )
    .emit(range(0, 4), 1)
    .finish()
    .unwrap();
    validate_projection_relation(&input, &final_output).unwrap();
}

#[test]
fn line_gate_proves_mutable_tail_and_sealing_convergence() {
    let first = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(6),
        StreamOffset::new(9),
        false,
    )
    .emit(range(0, 6), "hello")
    .emit(range(6, 9), "wor")
    .finish()
    .unwrap();
    let second = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(12),
        StreamOffset::new(12),
        true,
    )
    .emit(range(0, 6), "hello")
    .emit(range(6, 12), "world")
    .finish()
    .unwrap();
    validate_projection_transition(&first, &second).unwrap();
    assert_eq!(second.spans()[1].values(), &["world"]);
}

struct LocalProjector(Rc<RefCell<u32>>);

impl Projector<u8> for LocalProjector {
    type Output = u8;
    type Error = std::convert::Infallible;

    fn project(&mut self, input: &Projection<u8>) -> Result<Projection<u8>, Self::Error> {
        *self.0.borrow_mut() += 1;
        Ok(ProjectionBuilder::new(
            input.source_base(),
            input.stable_through(),
            input.source_end(),
            input.is_sealed(),
        )
        .emit_many(
            range(input.source_base().as_u64(), input.source_end().as_u64()),
            input
                .spans()
                .iter()
                .flat_map(|span| span.values().iter().copied()),
        )
        .finish()
        .expect("cloned projection remains valid"))
    }
}

#[test]
fn projector_may_be_non_send() {
    let counter = Rc::new(RefCell::new(0));
    let mut projector = LocalProjector(counter.clone());
    let input = complete::<u8>(4, true)
        .emit(range(0, 4), 1)
        .finish()
        .unwrap();
    projector.project(&input).unwrap();
    assert_eq!(*counter.borrow(), 1);
}
