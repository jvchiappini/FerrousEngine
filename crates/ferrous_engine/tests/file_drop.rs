//! File-drag events are plain values the application can match on.

use ferrous_engine::{FileDropEvent, FileDropKind};

#[test]
fn kinds_and_paths_are_preserved() {
    let entered = FileDropEvent::entered("C:/work/logo.png");
    assert_eq!(entered.kind, FileDropKind::Entered);
    assert_eq!(entered.kind.name(), "entered");
    assert_eq!(entered.path.to_string_lossy(), "C:/work/logo.png");

    let left = FileDropEvent::left("C:/work/logo.png");
    assert_eq!(left.kind, FileDropKind::Left);
    assert_eq!(left.kind.name(), "left");

    let dropped = FileDropEvent::dropped("C:/work/logo.png");
    assert_eq!(dropped.kind, FileDropKind::Dropped);
    assert_eq!(dropped.kind.name(), "dropped");
    assert_ne!(entered, dropped);
}
