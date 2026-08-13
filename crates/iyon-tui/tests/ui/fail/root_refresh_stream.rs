use iyon_tui::{History, TextStream};

fn main() {
    let mut history = History::new();
    let stream = history.push_stream(TextStream::new()).unwrap();
    let _ = history.refresh_stream(stream);
}
