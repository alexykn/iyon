use std::{cell::RefCell, rc::Rc};

use iyon_tui::{
    Projection, ProjectionBuilder, Projector, ProjectorExt, StreamOffset, StreamRange,
    validate_projection_relation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Record(&'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fragment(String);

#[derive(Debug)]
struct ToFragments {
    calls: Rc<RefCell<u32>>,
}

impl Projector<Record> for ToFragments {
    type Output = Fragment;
    type Error = std::convert::Infallible;

    fn project(
        &mut self,
        input: &Projection<Record>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        *self.calls.borrow_mut() += 1;
        let mut builder = ProjectionBuilder::new(
            input.source_base(),
            input.stable_through(),
            input.source_end(),
            input.is_sealed(),
        );
        for span in input.spans() {
            builder = builder.emit_many(
                span.source(),
                span.values()
                    .iter()
                    .map(|record| Fragment(record.0.to_owned())),
            );
        }
        Ok(builder.finish().unwrap())
    }
}

struct ToLines;

impl Projector<Fragment> for ToLines {
    type Output = String;
    type Error = std::convert::Infallible;

    fn project(
        &mut self,
        input: &Projection<Fragment>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        let mut builder = ProjectionBuilder::new(
            input.source_base(),
            input.stable_through(),
            input.source_end(),
            input.is_sealed(),
        );
        for span in input.spans() {
            builder = builder.emit_many(
                span.source(),
                span.values().iter().map(|fragment| fragment.0.clone()),
            );
        }
        Ok(builder.finish().unwrap())
    }

    fn restart_from(&self, output_from: StreamOffset) -> StreamOffset {
        output_from
            .as_u64()
            .checked_sub(1)
            .map(StreamOffset::new)
            .unwrap_or(StreamOffset::ZERO)
    }
}

fn source() -> Projection<Record> {
    ProjectionBuilder::new(
        StreamOffset::new(10),
        StreamOffset::new(12),
        StreamOffset::new(12),
        true,
    )
    .emit(
        StreamRange::new(StreamOffset::new(10), StreamOffset::new(11)),
        Record("log"),
    )
    .elide(StreamRange::new(
        StreamOffset::new(11),
        StreamOffset::new(12),
    ))
    .finish()
    .unwrap()
}

#[test]
fn external_consumer_can_build_and_compose_projections() {
    let calls = Rc::new(RefCell::new(0));
    let projector = ToFragments {
        calls: calls.clone(),
    }
    .then(ToLines);
    let mut projector = projector;
    let output = projector.project(&source()).unwrap();

    assert_eq!(*calls.borrow(), 1);
    assert_eq!(output.source_base(), StreamOffset::new(10));
    assert_eq!(output.source_end(), StreamOffset::new(12));
    assert_eq!(output.spans()[0].values(), &["log".to_owned()]);
    assert!(output.spans()[1].values().is_empty());
    validate_projection_relation(&source(), &output).unwrap();
}

#[test]
fn non_send_projector_is_usable_and_restart_backchains() {
    let projector = ToFragments {
        calls: Rc::new(RefCell::new(0)),
    }
    .then(ToLines);
    assert_eq!(
        projector.restart_from(StreamOffset::new(10)),
        StreamOffset::new(9)
    );
}

#[test]
fn fields_are_exposed_only_through_accessors() {
    let projection = source();
    assert_eq!(projection.spans().len(), 2);
    assert_eq!(projection.spans()[0].source().len(), 1);
}

#[test]
fn arbitrary_coordinate_replacement_is_not_byte_sliced() {
    let range = StreamRange::new(StreamOffset::new(10), StreamOffset::new(11));
    let snapshot = iyon_tui::StreamSnapshotBuilder::new(
        iyon_tui::StreamRevision::ZERO,
        range.start(),
        range.end(),
        range.end(),
    )
    .projected_text(
        iyon_tui::ProjectedText::builder(range)
            .replacement("event 10 finished", range, iyon_tui::StyleSpec::new())
            .finish()
            .unwrap(),
    )
    .finish()
    .unwrap();
    assert_eq!(snapshot.source_end(), StreamOffset::new(11));
}
