use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use iyon_tui::{
    Component, ComponentCx, ComponentHandle, FlowBoundary, History, HistoryError, HistoryLayout,
    HistoryUnitId, Insets, IntoView, StreamOffset, StreamRange, StreamRevision, StreamSnapshot,
    StreamSnapshotBuilder, StreamingSource, TextSpan, View,
};

#[derive(Debug)]
struct PublicComponent;

impl Component for PublicComponent {
    fn view(&self) -> View {
        View::text("component").into_view()
    }

    fn capabilities(&self, _cx: &mut ComponentCx<'_, Self>) {}
}

fn live_signature<C: Component>(handle: ComponentHandle<C>) -> Result<HistoryUnitId, HistoryError> {
    let mut history = History::new();
    let unit = history.push(View::component(handle))?;
    history.freeze(unit, "finished")?;
    Ok(unit)
}

#[derive(Clone)]
struct LocalSource {
    text: Rc<RefCell<String>>,
    revision: Cell<u64>,
    sealed: Cell<bool>,
}

impl LocalSource {
    fn new(text: &str) -> Self {
        Self {
            text: Rc::new(RefCell::new(text.to_owned())),
            revision: Cell::new(0),
            sealed: Cell::new(false),
        }
    }

    fn append(&mut self, text: &str) {
        self.text.borrow_mut().push_str(text);
        self.revision.set(self.revision.get().saturating_add(1));
    }
}

impl StreamingSource for LocalSource {
    fn snapshot(&self) -> StreamSnapshot {
        let text = self.text.borrow().clone();
        let end = text.len() as u64;
        StreamSnapshotBuilder::new(
            StreamRevision::new(self.revision.get()),
            StreamOffset::ZERO,
            StreamOffset::new(end),
            StreamOffset::new(end),
        )
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(end)),
            [TextSpan::plain(text)],
        )
        .finish()
        .expect("valid local snapshot")
    }

    fn seal(&mut self) {
        self.sealed.set(true);
        self.revision.set(self.revision.get().saturating_add(1));
    }

    fn is_sealed(&self) -> bool {
        self.sealed.get()
    }
}

#[test]
fn public_history_static_live_layout_and_boundary_api() {
    let mut history = History::default();
    let layout = HistoryLayout::new(Insets::all(1), 2);
    history.set_layout(layout);
    assert_eq!(history.layout(), layout);
    assert_eq!(history.layout().padding(), Insets::all(1));
    assert_eq!(history.layout().gap(), 2);

    let first = history.push("A").unwrap();
    let attached = history
        .push_with_boundary("B", FlowBoundary::AttachToPrevious)
        .unwrap();
    assert_ne!(first, attached);
    assert_eq!(first, first);
    let copied = first;
    assert_eq!(copied, first);
    assert!(format!("{first:?}").contains("HistoryUnitId"));
}

#[test]
fn public_typed_stream_supports_non_send_update_refresh_seal_and_append() {
    let mut history = History::new();
    let handle = history.push_stream(LocalSource::new("A")).unwrap();
    assert_eq!(handle.unit(), handle.unit());
    history
        .update_stream(handle, |source| source.append("B"))
        .unwrap();
    history.refresh_stream(handle).unwrap();
    history.seal_stream(handle).unwrap();
    history.push("after").unwrap();
}

#[test]
fn stale_stream_handle_is_safe_across_histories() {
    let mut first = History::new();
    let handle = first.push_stream(LocalSource::new("A")).unwrap();
    drop(first);

    let mut second = History::new();
    assert!(matches!(
        second.refresh_stream(handle),
        Err(HistoryError::UnitNotFound { unit }) if unit == handle.unit()
    ));
}

#[test]
fn public_error_has_standard_error_contract() {
    fn assert_error<E: std::error::Error>() {}
    assert_error::<HistoryError>();
    let _ = live_signature::<PublicComponent>;
}
