use super::*;
use crate::geometry::Size;

#[test]
fn transparent_and_blank_cells_remain_distinct() {
    let transparent = PhysicalCell::transparent();
    let blank = PhysicalCell::blank(PhysicalStyle {
        background: Some(PhysicalColor::Indexed(1)),
        ..PhysicalStyle::default()
    });

    assert!(!transparent.painted);
    assert!(blank.painted);
    assert_ne!(transparent, blank);
}

#[test]
fn surfaces_support_zero_width_and_zero_height_geometry() {
    for (width, height) in [(0, 0), (0, 3), (4, 0)] {
        let surface = Surface::new(width, height);
        assert_eq!(surface.size, Size { width, height });
        assert_eq!(
            surface.cells.len(),
            usize::from(width) * usize::from(height)
        );
    }
}

#[test]
fn composite_preserves_transparency_and_incompleteness() {
    let mut parent = Surface::new(2, 1);
    let mut child = Surface::new(2, 1);
    child.physically_complete = false;
    child.get_mut(0, 0).grapheme = Some("x".to_string());
    child.get_mut(0, 0).painted = true;

    parent.composite(&child, 0, 0);

    assert!(!parent.physically_complete);
    assert!(parent.get(0, 0).painted);
    assert!(!parent.get(1, 0).painted);
}

#[test]
fn wide_continuation_is_occupied_but_not_text() {
    let row = PhysicalRow::from_cells(vec![
        PhysicalCell {
            grapheme: Some("界".to_string()),
            style: PhysicalStyle::default(),
            painted: true,
            continuation: false,
        },
        PhysicalCell {
            grapheme: None,
            style: PhysicalStyle::default(),
            painted: true,
            continuation: true,
        },
    ]);

    assert_eq!(row.width(), 2);
    assert_eq!(row.plain_text(), "界");
}
