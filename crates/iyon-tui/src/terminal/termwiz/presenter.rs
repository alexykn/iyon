use anyhow::{Result, anyhow};
use termwiz::{
    cell::CellAttributes,
    surface::{Change, CursorVisibility, Position, Surface},
    terminal::Terminal,
};

use crate::physical::PhysicalRow;

use super::lower::row_changes;

pub(crate) struct TermwizPresenter {
    presented: Surface,
    known: bool,
}

impl TermwizPresenter {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            presented: Surface::new(width, height),
            known: false,
        }
    }

    pub(crate) fn dimensions(&self) -> (usize, usize) {
        self.presented.dimensions()
    }

    pub(crate) fn resize(&mut self, width: usize, height: usize) {
        if self.dimensions() == (width, height) {
            return;
        }
        self.presented.resize(width, height);
        self.known = false;
    }

    pub(crate) fn present<T: Terminal + ?Sized>(
        &mut self,
        terminal: &mut T,
        desired: Surface,
    ) -> Result<()> {
        if desired.dimensions() != self.dimensions() {
            let (width, height) = desired.dimensions();
            self.resize(width, height);
        }

        if !self.known {
            let changes = full_repaint_changes(&desired);
            return self.apply(terminal, changes, desired);
        }

        let changes = self.presented.diff_screens(&desired);
        if changes.is_empty() {
            return Ok(());
        }
        self.apply(terminal, with_hidden_cursor(changes, &desired), desired)
    }

    pub(crate) fn insert_history<T: Terminal + ?Sized>(
        &mut self,
        terminal: &mut T,
        rows: &[PhysicalRow],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let (width, height) = self.dimensions();
        if width == 0 || height == 0 {
            return Ok(0);
        }
        if !self.known {
            let retained = self.presented.clone();
            let changes = full_repaint_changes(&retained);
            self.apply(terminal, changes, retained)?;
        }

        let transaction = native_transaction(&self.presented, rows);

        match terminal.render(&transaction).and_then(|_| terminal.flush()) {
            Ok(()) => Ok(rows.len()),
            Err(error) => {
                self.known = false;
                Err(anyhow!(error))
            }
        }
    }

    pub(crate) fn position_after_final_frame<T: Terminal + ?Sized>(
        &mut self,
        terminal: &mut T,
    ) -> Result<()> {
        let (_, height) = self.dimensions();
        let changes = canonical_cursor(height);
        terminal.render(&changes)?;
        terminal.flush()?;
        Ok(())
    }

    fn apply<T: Terminal + ?Sized>(
        &mut self,
        terminal: &mut T,
        changes: Vec<Change>,
        desired: Surface,
    ) -> Result<()> {
        if let Err(error) = terminal.render(&changes).and_then(|_| terminal.flush()) {
            self.known = false;
            return Err(anyhow!(error));
        }
        self.presented = desired;
        self.known = true;
        Ok(())
    }
}

fn native_transaction(presented: &Surface, rows: &[PhysicalRow]) -> Vec<Change> {
    let (_, height) = presented.dimensions();
    let mut transaction = Vec::new();
    for chunk in rows.chunks(height) {
        for (row_index, row) in chunk.iter().enumerate() {
            transaction.extend(row_changes(row, row_index, true));
        }
        transaction.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(height - 1),
        });
        transaction.push(Change::Text("\r\n".repeat(chunk.len())));
    }

    // Native scrolling invalidates the terminal's active-screen coordinates.
    // Repaint every retained row before acknowledging the transfer instead of
    // attempting to infer the terminal's post-scroll screen from a model.
    transaction.extend(full_repaint_changes(presented));
    transaction
}

fn full_repaint_changes(surface: &Surface) -> Vec<Change> {
    let (width, height) = surface.dimensions();
    let mut changes = vec![Change::CursorVisibility(CursorVisibility::Hidden)];
    for (y, line) in surface.screen_lines().into_iter().enumerate() {
        changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(y),
        });
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::ClearToEndOfLine(Default::default()));
        for cell in line.visible_cells() {
            let x = cell.cell_index();
            if x >= width {
                continue;
            }
            changes.push(Change::CursorPosition {
                x: Position::Absolute(x),
                y: Position::Absolute(y),
            });
            changes.push(Change::AllAttributes(cell.attrs().clone()));
            changes.push(Change::Text(cell.str().to_string()));
        }
    }
    changes.extend(canonical_cursor(height));
    changes
}

fn with_hidden_cursor(mut changes: Vec<Change>, desired: &Surface) -> Vec<Change> {
    changes.extend(canonical_cursor(desired.dimensions().1));
    changes
}

fn canonical_cursor(height: usize) -> Vec<Change> {
    let y = height.saturating_sub(1);
    vec![
        Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(y),
        },
        Change::CursorVisibility(CursorVisibility::Hidden),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::{PhysicalCell, PhysicalColor, PhysicalRow, PhysicalStyle};
    use termwiz::surface::Change;

    fn surface(text: &str, width: usize, height: usize) -> Surface {
        let mut surface = Surface::new(width, height);
        surface.add_changes(vec![
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            Change::Text(text.to_string()),
        ]);
        surface
    }

    #[test]
    fn identical_surfaces_have_no_diff() {
        let a = surface("hello", 5, 1);
        assert!(a.diff_screens(&a).is_empty());
    }

    #[test]
    fn applying_a_diff_reaches_the_desired_surface() {
        let a = surface("hello", 5, 1);
        let b = surface("x    ", 5, 1);
        let changes = a.diff_screens(&b);
        let mut actual = a.clone();
        actual.add_changes(changes);
        assert_eq!(actual.screen_chars_to_string(), b.screen_chars_to_string());
    }

    #[test]
    fn native_transaction_repairs_the_active_screen_without_region_scrolls() {
        let presented = surface("hello", 5, 2);
        let row = PhysicalRow::from_cells(vec![
            PhysicalCell {
                grapheme: Some("a".to_string()),
                style: Default::default(),
                painted: true,
                continuation: false,
            },
            PhysicalCell {
                grapheme: Some("b".to_string()),
                style: Default::default(),
                painted: true,
                continuation: false,
            },
            PhysicalCell {
                grapheme: Some("c".to_string()),
                style: Default::default(),
                painted: true,
                continuation: false,
            },
        ]);
        let transaction = native_transaction(&presented, &[row]);
        assert!(!transaction.iter().any(|change| matches!(
            change,
            Change::ScrollRegionUp { .. } | Change::ScrollRegionDown { .. }
        )));

        let mut actual = presented.clone();
        actual.add_changes(transaction);
        assert_eq!(
            actual.screen_chars_to_string(),
            presented.screen_chars_to_string()
        );
    }

    fn physical_surface(rows: Vec<Vec<PhysicalCell>>, width: u16) -> Surface {
        let mut surface = Surface::new(usize::from(width), rows.len());
        for (y, cells) in rows.into_iter().enumerate() {
            surface.add_changes(row_changes(&PhysicalRow::from_cells(cells), y, true));
        }
        surface
    }

    fn painted_row(text: &str, width: usize, style: PhysicalStyle) -> Vec<PhysicalCell> {
        let mut cells = text
            .chars()
            .take(width)
            .map(|character| PhysicalCell {
                grapheme: Some(character.to_string()),
                style,
                painted: true,
                continuation: false,
            })
            .collect::<Vec<_>>();
        while cells.len() < width {
            cells.push(PhysicalCell {
                grapheme: Some(" ".into()),
                style,
                painted: true,
                continuation: false,
            });
        }
        cells
    }

    fn assert_virtual_screen_matches_surface(model: &VirtualTerminal, expected: &Surface) {
        assert_eq!((model.width, model.height), expected.dimensions());
        for (y, line) in expected.screen_lines().iter().enumerate() {
            for x in 0..model.width {
                let expected_cell = line.get_cell(x).expect("expected cell");
                let actual = &model.screen[y][x];
                assert_eq!(actual.text, expected_cell.str());
                assert_eq!(actual.attrs, *expected_cell.attrs());
            }
        }
    }

    fn assert_surface_state_equal(actual: &Surface, expected: &Surface) {
        assert_eq!(actual.dimensions(), expected.dimensions());
        let actual_lines = actual.screen_lines();
        let expected_lines = expected.screen_lines();
        for (actual_line, expected_line) in actual_lines.iter().zip(expected_lines.iter()) {
            for x in 0..actual.dimensions().0 {
                let actual_cell = actual_line.get_cell(x).expect("actual cell");
                let expected_cell = expected_line.get_cell(x).expect("expected cell");
                assert_eq!(actual_cell.str(), expected_cell.str());
                assert_eq!(actual_cell.attrs(), expected_cell.attrs());
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct VirtualCell {
        text: String,
        attrs: CellAttributes,
    }

    struct VirtualTerminal {
        width: usize,
        height: usize,
        screen: Vec<Vec<VirtualCell>>,
        scrollback: Vec<Vec<VirtualCell>>,
        x: usize,
        y: usize,
        attrs: CellAttributes,
    }

    impl VirtualTerminal {
        fn new(width: usize, height: usize) -> Self {
            let blank = || VirtualCell {
                text: " ".into(),
                attrs: CellAttributes::default(),
            };
            Self {
                width,
                height,
                screen: (0..height)
                    .map(|_| (0..width).map(|_| blank()).collect())
                    .collect(),
                scrollback: Vec::new(),
                x: 0,
                y: 0,
                attrs: CellAttributes::default(),
            }
        }

        fn scroll_up(&mut self) {
            if self.height == 0 {
                return;
            }
            self.scrollback.push(self.screen.remove(0));
            self.screen.push(
                (0..self.width)
                    .map(|_| VirtualCell {
                        text: " ".into(),
                        attrs: CellAttributes::default(),
                    })
                    .collect(),
            );
            self.y = self.height - 1;
        }

        fn apply(&mut self, changes: &[Change]) {
            for change in changes {
                match change {
                    Change::CursorPosition { x, y } => {
                        self.x = match x {
                            Position::Absolute(value) => *value,
                            _ => panic!("test model only supports absolute x"),
                        };
                        self.y = match y {
                            Position::Absolute(value) => *value,
                            _ => panic!("test model only supports absolute y"),
                        };
                    }
                    Change::AllAttributes(attrs) => self.attrs = attrs.clone(),
                    Change::ClearToEndOfLine(_) => {
                        for cell in self.screen[self.y][self.x..].iter_mut() {
                            *cell = VirtualCell {
                                text: " ".into(),
                                attrs: self.attrs.clone(),
                            };
                        }
                    }
                    Change::Text(text) => {
                        for character in text.chars() {
                            match character {
                                '\r' => self.x = 0,
                                '\n' => {
                                    if self.y + 1 >= self.height {
                                        self.scroll_up();
                                    } else {
                                        self.y += 1;
                                    }
                                }
                                character => {
                                    if self.x < self.width && self.y < self.height {
                                        self.screen[self.y][self.x] = VirtualCell {
                                            text: character.to_string(),
                                            attrs: self.attrs.clone(),
                                        };
                                    }
                                    self.x += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        fn row_text(row: &[VirtualCell]) -> String {
            row.iter().map(|cell| cell.text.as_str()).collect()
        }
    }

    #[test]
    fn native_transaction_repairs_a_styled_active_screen_in_an_independent_model() {
        let width = 8;
        let bubble_style = PhysicalStyle {
            background: Some(PhysicalColor::Indexed(4)),
            ..PhysicalStyle::default()
        };
        let presented = physical_surface(
            vec![
                painted_row("USER", width, bubble_style),
                painted_row("USER", width, bubble_style),
                painted_row("model", width, PhysicalStyle::default()),
                vec![PhysicalCell::transparent(); width],
                painted_row("--------", width, PhysicalStyle::default()),
                painted_row("input", width, PhysicalStyle::default()),
                painted_row("--------", width, PhysicalStyle::default()),
                painted_row("status", width, PhysicalStyle::default()),
            ],
            width as u16,
        );
        let rows = [
            PhysicalRow::from_cells(vec![PhysicalCell {
                grapheme: Some("a".into()),
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            }]),
            PhysicalRow::from_cells(Vec::new()),
            PhysicalRow::from_cells(vec![PhysicalCell {
                grapheme: Some("b".into()),
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            }]),
        ];
        let transaction = native_transaction(&presented, &rows);
        let mut model = VirtualTerminal::new(width, 8);
        model.apply(&transaction);

        assert_eq!(
            model
                .scrollback
                .iter()
                .map(|row| VirtualTerminal::row_text(row))
                .collect::<Vec<_>>(),
            ["a       ", "        ", "b       "]
        );
        assert_virtual_screen_matches_surface(&model, &presented);

        let next = physical_surface(
            vec![
                painted_row("model", width, PhysicalStyle::default()),
                vec![PhysicalCell::transparent(); width],
                painted_row("--------", width, PhysicalStyle::default()),
                painted_row("input", width, PhysicalStyle::default()),
                painted_row("--------", width, PhysicalStyle::default()),
                painted_row("status", width, PhysicalStyle::default()),
                vec![PhysicalCell::transparent(); width],
                vec![PhysicalCell::transparent(); width],
            ],
            width as u16,
        );
        let mut actual = presented.clone();
        actual.add_changes(transaction);
        actual.add_changes(actual.diff_screens(&next));
        assert_surface_state_equal(&actual, &next);
    }

    #[test]
    fn differential_present_clears_styled_bubble_cells_and_attributes() {
        let bubble_style = PhysicalStyle {
            foreground: Some(PhysicalColor::Indexed(4)),
            background: Some(PhysicalColor::Indexed(7)),
            bold: true,
            italic: true,
            underline: true,
            ..PhysicalStyle::default()
        };
        let a = physical_surface(
            vec![vec![
                PhysicalCell {
                    grapheme: Some("A".into()),
                    style: bubble_style,
                    painted: true,
                    continuation: false,
                },
                PhysicalCell {
                    grapheme: Some("B".into()),
                    style: bubble_style,
                    painted: true,
                    continuation: false,
                },
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
            ]],
            4,
        );
        let b = physical_surface(
            vec![vec![
                PhysicalCell {
                    grapheme: Some("x".into()),
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                },
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
            ]],
            4,
        );
        let mut actual = a.clone();
        let changes = actual.diff_screens(&b);
        actual.add_changes(changes);
        assert_surface_state_equal(&actual, &b);
    }

    #[test]
    fn native_repair_then_changed_frame_reaches_exact_styled_state() {
        let a = physical_surface(
            vec![vec![
                PhysicalCell {
                    grapheme: Some("A".into()),
                    style: PhysicalStyle {
                        background: Some(PhysicalColor::Indexed(2)),
                        ..PhysicalStyle::default()
                    },
                    painted: true,
                    continuation: false,
                },
                PhysicalCell {
                    grapheme: Some("B".into()),
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                },
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
            ]],
            4,
        );
        let native = PhysicalRow::from_cells(vec![PhysicalCell {
            grapheme: Some("n".into()),
            style: PhysicalStyle::default(),
            painted: true,
            continuation: false,
        }]);
        let mut actual = a.clone();
        actual.add_changes(native_transaction(&a, &[native]));
        assert_surface_state_equal(&actual, &a);

        let b = physical_surface(
            vec![vec![
                PhysicalCell {
                    grapheme: Some("z".into()),
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                },
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
                PhysicalCell::transparent(),
            ]],
            4,
        );
        let changes = actual.diff_screens(&b);
        actual.add_changes(changes);
        assert_surface_state_equal(&actual, &b);
    }

    #[test]
    fn resize_preserves_retained_overlap_for_native_repair() {
        let mut retained = physical_surface(
            vec![vec![
                PhysicalCell {
                    grapheme: Some("A".into()),
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                },
                PhysicalCell {
                    grapheme: Some("B".into()),
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                },
            ]],
            2,
        );
        retained.resize(3, 2);
        let expected = retained.clone();
        retained.add_changes(native_transaction(
            &expected,
            &[PhysicalRow::from_cells(vec![PhysicalCell {
                grapheme: Some("n".into()),
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            }])],
        ));
        assert_surface_state_equal(&retained, &expected);
    }

    #[test]
    fn native_transaction_handles_batches_larger_than_the_screen() {
        let presented = surface("1234", 4, 2);
        let rows = (0..5)
            .map(|index| {
                PhysicalRow::from_cells(vec![PhysicalCell {
                    grapheme: Some(char::from(b'a' + index).to_string()),
                    style: Default::default(),
                    painted: true,
                    continuation: false,
                }])
            })
            .collect::<Vec<_>>();
        let mut actual = presented.clone();
        actual.add_changes(native_transaction(&presented, &rows));
        assert_eq!(
            actual.screen_chars_to_string(),
            presented.screen_chars_to_string()
        );
    }
}
