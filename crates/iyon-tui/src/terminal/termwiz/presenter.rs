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
        self.presented = Surface::new(width, height);
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
            let blank = Surface::new(width, height);
            let changes = full_repaint_changes(&blank);
            self.apply(terminal, changes, blank)?;
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
    let mut disturbed = presented.clone();
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

    disturbed.add_changes(transaction.clone());
    let repair = disturbed.diff_screens(presented);
    transaction.extend(repair);
    transaction.extend(canonical_cursor(height));
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
    use crate::physical::{PhysicalCell, PhysicalRow};
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
