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
        .emit(range(0, 1), 1)
        .emit(range(1, 2), 2)
        .elide(range(2, 4))
        .finish()
        .unwrap();
    let next = complete::<u8>(4, true)
        .emit_many(range(0, 2), [1, 2])
        .emit_many(range(2, 4), [2, 3])
        .finish()
        .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &next),
        Err(ProjectionTransitionError::StablePrefixChanged)
    );

    let tail_replaced = complete::<u8>(2, false)
        .emit(range(0, 1), 1)
        .emit(range(1, 2), 2)
        .emit_many(range(2, 4), [2, 3])
        .finish()
        .unwrap();
    validate_projection_transition(&previous, &tail_replaced).unwrap();

    let changed = complete::<u8>(2, false)
        .emit(range(0, 1), 9)
        .emit(range(1, 2), 2)
        .elide(range(2, 4))
        .finish()
        .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &changed),
        Err(ProjectionTransitionError::StablePrefixChanged)
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
    .emit_many(range(0, 2), [1, 1])
    .finish()
    .unwrap();
    validate_projection_relation(&input, &output).unwrap();
    let final_output = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        true,
    )
    .emit_many(range(0, 4), [1, 1])
    .finish()
    .unwrap();
    validate_projection_relation(&input, &final_output).unwrap();

    let base_mismatch = ProjectionBuilder::new(
        StreamOffset::new(1),
        StreamOffset::new(1),
        StreamOffset::new(2),
        false,
    )
    .emit(range(1, 2), 1)
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_relation(&input, &base_mismatch),
        Err(ProjectionRelationError::SourceBaseMismatch)
    );
    let beyond_end = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(5),
        false,
    )
    .emit(range(0, 4), 1)
    .emit(range(4, 5), 1)
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_relation(&input, &beyond_end),
        Err(ProjectionRelationError::OutputEndBeyondInput)
    );
    let stability_input = ProjectionBuilder::<u8>::new(
        StreamOffset::ZERO,
        StreamOffset::new(2),
        StreamOffset::new(4),
        false,
    )
    .emit(range(0, 2), 1)
    .elide(range(2, 4))
    .finish()
    .unwrap();
    let beyond_stability = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(3),
        StreamOffset::new(4),
        false,
    )
    .emit(range(0, 3), 1)
    .elide(range(3, 4))
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_relation(&stability_input, &beyond_stability),
        Err(ProjectionRelationError::OutputStabilityBeyondInput)
    );
    let sealed_open_input = ProjectionBuilder::<u8>::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        false,
    )
    .emit(range(0, 4), 1)
    .finish()
    .unwrap();
    let sealed_output = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        true,
    )
    .emit(range(0, 4), 1)
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_relation(&sealed_open_input, &sealed_output),
        Err(ProjectionRelationError::OutputSealedBeforeInput)
    );
    let sealed_lag = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(2),
        StreamOffset::new(2),
        true,
    )
    .emit(range(0, 2), 1)
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_relation(&input, &sealed_lag),
        Err(ProjectionRelationError::SealedOutputNotCaughtUp)
    );
}

struct LineGate;

impl Projector<char> for LineGate {
    type Output = String;
    type Error = std::convert::Infallible;

    fn project(
        &mut self,
        input: &Projection<char>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        let chars = input
            .spans()
            .iter()
            .flat_map(|span| span.values().iter().copied());
        let mut output = ProjectionBuilder::new(
            input.source_base(),
            input.source_base(),
            input.source_end(),
            false,
        );
        let mut start = input.source_base();
        let mut line = String::new();
        let mut cursor = start;
        for character in chars {
            cursor = cursor.saturating_add(1);
            line.push(character);
            if character == '\n' {
                output = output.emit(
                    range(start.as_u64(), cursor.as_u64()),
                    line.trim_end_matches('\n').to_owned(),
                );
                line.clear();
                start = cursor;
            }
        }
        if start < cursor {
            output = output.emit(range(start.as_u64(), cursor.as_u64()), line);
        }
        let stable = if input.is_sealed() {
            input.source_end()
        } else {
            start
        };
        let mut final_output = ProjectionBuilder::new(
            input.source_base(),
            stable,
            input.source_end(),
            input.is_sealed(),
        );
        for span in output.finish().unwrap().spans() {
            final_output = final_output.emit_many(span.source(), span.values().iter().cloned());
        }
        Ok(final_output.finish().unwrap())
    }
}

#[test]
fn line_gate_projector_converges_incremental_and_batch() {
    let open = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(6),
        StreamOffset::new(9),
        false,
    )
    .emit_many(range(0, 6), "hello\n".chars())
    .emit_many(range(6, 9), "wor".chars())
    .finish()
    .unwrap();
    let sealed = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(12),
        StreamOffset::new(12),
        true,
    )
    .emit_many(range(0, 12), "hello\nworld!".chars())
    .finish()
    .unwrap();
    let mut gate = LineGate;
    let mut batch = LineGate;
    let incremental = gate.project(&sealed).unwrap();
    let one_shot = batch.project(&sealed).unwrap();
    assert_eq!(incremental, one_shot);
    let open_output = gate.project(&open).unwrap();
    assert_eq!(open_output.spans()[0].values(), &["hello"]);
    validate_projection_transition(&open_output, &incremental).unwrap();
}

#[test]
fn transitions_reject_monotonicity_violations_and_sealed_mutations() {
    let previous = ProjectionBuilder::new(
        StreamOffset::new(1),
        StreamOffset::new(2),
        StreamOffset::new(4),
        false,
    )
    .emit(range(1, 2), 1)
    .elide(range(2, 4))
    .finish()
    .unwrap();
    let base_regressed = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(2),
        StreamOffset::new(4),
        false,
    )
    .emit(range(0, 2), 1)
    .elide(range(2, 4))
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &base_regressed),
        Err(ProjectionTransitionError::SourceBaseRegressed)
    );
    let end_regressed = ProjectionBuilder::new(
        StreamOffset::new(1),
        StreamOffset::new(2),
        StreamOffset::new(3),
        false,
    )
    .emit(range(1, 2), 1)
    .elide(range(2, 3))
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &end_regressed),
        Err(ProjectionTransitionError::SourceEndRegressed)
    );
    let stability_regressed = ProjectionBuilder::new(
        StreamOffset::new(1),
        StreamOffset::new(1),
        StreamOffset::new(4),
        false,
    )
    .elide(range(1, 4))
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_transition(&previous, &stability_regressed),
        Err(ProjectionTransitionError::StabilityRegressed)
    );

    let sealed = complete::<u8>(4, true)
        .emit(range(0, 4), 1)
        .finish()
        .unwrap();
    let unsealed = ProjectionBuilder::new(
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
        false,
    )
    .emit(range(0, 4), 1)
    .finish()
    .unwrap();
    assert_eq!(
        validate_projection_transition(&sealed, &unsealed),
        Err(ProjectionTransitionError::UnsealedAfterSeal)
    );
    let changed = complete::<u8>(4, true)
        .emit(range(0, 4), 2)
        .finish()
        .unwrap();
    assert_eq!(
        validate_projection_transition(&sealed, &changed),
        Err(ProjectionTransitionError::ChangedAfterSeal)
    );
    validate_projection_transition(&sealed, &sealed).unwrap();
}

struct FailFirst;
struct FailSecond;
struct BadRelation;

#[derive(Debug, Clone, Copy)]
struct Failure;
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failure")
    }
}
impl std::error::Error for Failure {}

impl Projector<u8> for FailFirst {
    type Output = u8;
    type Error = Failure;
    fn project(&mut self, _: &Projection<u8>) -> Result<Projection<u8>, Self::Error> {
        Err(Failure)
    }
}
impl Projector<u8> for FailSecond {
    type Output = u8;
    type Error = Failure;
    fn project(&mut self, _: &Projection<u8>) -> Result<Projection<u8>, Self::Error> {
        Err(Failure)
    }
}
impl Projector<u8> for BadRelation {
    type Output = u8;
    type Error = Failure;
    fn project(&mut self, input: &Projection<u8>) -> Result<Projection<u8>, Self::Error> {
        Ok(ProjectionBuilder::new(
            StreamOffset::new(1),
            input.stable_through(),
            input.source_end(),
            input.is_sealed(),
        )
        .emit(range(1, input.source_end().as_u64()), 1)
        .finish()
        .unwrap())
    }
}

struct Identity;

impl Projector<u8> for Identity {
    type Output = u8;
    type Error = Failure;

    fn project(&mut self, input: &Projection<u8>) -> Result<Projection<u8>, Self::Error> {
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
        .unwrap())
    }
}

#[test]
fn composition_reports_each_stage_and_contract() {
    let input = complete::<u8>(4, true)
        .emit(range(0, 4), 1)
        .finish()
        .unwrap();
    let mut first_error = FailFirst.then(Identity);
    assert!(matches!(
        first_error.project(&input),
        Err(ThenError::First(_))
    ));
    let mut first_relation = BadRelation.then(Identity);
    assert!(matches!(
        first_relation.project(&input),
        Err(ThenError::FirstRelation(_))
    ));
    let mut second_error = Identity.then(FailSecond);
    assert!(matches!(
        second_error.project(&input),
        Err(ThenError::Second(_))
    ));
    let mut second_relation = Identity.then(BadRelation);
    assert!(matches!(
        second_relation.project(&input),
        Err(ThenError::SecondRelation(_))
    ));
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
