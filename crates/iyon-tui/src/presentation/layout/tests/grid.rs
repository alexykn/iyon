use super::*;
use crate::geometry::Rect;
use crate::presentation::{GridCellSpec, GridTrack, HorizontalAlign, Insets, VerticalAlign};

fn tree(view: &View, width: u16) -> LayoutTree {
    ViewCompiler::default().layout_tree(view, LayoutConstraints::width_only(width))
}

fn child_rects(view: &View, width: u16) -> Vec<Rect> {
    let laid_out = tree(view, width);
    let root = laid_out.node(laid_out.root);
    root.children
        .iter()
        .map(|id| laid_out.node(*id).rect)
        .collect()
}

fn text_at(view: &View, width: u16, needle: &str) -> (u16, u16) {
    let block = compile_view(view, width);
    for (y, row) in block.rows.iter().enumerate() {
        if let Some(x) = row.plain_text().find(needle) {
            return (x as u16, y as u16);
        }
    }
    panic!(
        "did not find {needle:?} in {:?}",
        block
            .rows
            .iter()
            .map(PhysicalRow::plain_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn shared_columns_align_across_rows() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::content(), GridTrack::content()]);
        grid.row(|row| {
            row.cell("a");
            row.cell("long-long");
        });
        grid.row(|row| {
            row.cell("longer");
            row.cell("x");
        });
    });
    let (x1, _) = text_at(&view, 40, "long-long");
    let (x2, _) = text_at(&view, 40, "x");
    assert_eq!(x1, x2);
    let rects = child_rects(&view, 40);
    assert_eq!(rects[1].x, rects[3].x);
}

#[test]
fn fixed_content_flex_consume_width() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::fixed(3), GridTrack::content(), GridTrack::flex()]);
        grid.row(|row| {
            row.cell("abc");
            row.cell("12345");
            row.cell(View::text("flex").fill_width());
        });
    })
    .fill_width();
    let rects = child_rects(&view, 20);
    assert_eq!(
        rects
            .iter()
            .map(|rect| (rect.x, rect.width))
            .collect::<Vec<_>>(),
        vec![(0, 3), (3, 5), (8, 12)],
        "rects={rects:?} tree_size={:?}",
        tree(&view, 20).size
    );
}

#[test]
fn wrapping_flex_cell_contributes_row_height() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::content(), GridTrack::flex()]);
        grid.row(|row| {
            row.cell("a");
            row.cell("word word word word");
        });
    })
    .fill_width();
    let block = compile_view(&view, 12);
    assert!(block.rows.len() > 1);
}

#[test]
fn fit_child_keeps_intrinsic_width_fill_uses_cell() {
    let fit = View::grid(|grid| {
        grid.columns([GridTrack::fixed(10)]);
        grid.row(|row| {
            row.cell("hi");
        });
    });
    let fill = View::grid(|grid| {
        grid.columns([GridTrack::fixed(10)]);
        grid.row(|row| {
            row.cell(View::text("hi").fill_width());
        });
    });
    assert_eq!(child_rects(&fit, 20)[0].width, 2);
    assert_eq!(child_rects(&fill, 20)[0].width, 10);
}

#[test]
fn horizontal_alignment_places_the_child_view() {
    let view = View::grid(|grid| {
        grid.columns([
            GridTrack::fixed(8),
            GridTrack::fixed(8),
            GridTrack::fixed(8),
        ]);
        grid.row(|row| {
            row.cell_with(
                GridCellSpec::new().horizontal_align(HorizontalAlign::Start),
                "x",
            );
            row.cell_with(
                GridCellSpec::new().horizontal_align(HorizontalAlign::Center),
                "x",
            );
            row.cell_with(
                GridCellSpec::new().horizontal_align(HorizontalAlign::End),
                "x",
            );
        });
    });
    let rects = child_rects(&view, 24);
    assert_eq!(rects[0].x, 0);
    assert_eq!(rects[1].x, 8 + 3);
    assert_eq!(rects[2].x, 16 + 7);
}

#[test]
fn vertical_alignment_places_the_child_view() {
    let view = View::grid(|grid| {
        grid.columns([
            GridTrack::fixed(1),
            GridTrack::fixed(1),
            GridTrack::fixed(1),
        ]);
        grid.row_with(GridTrack::fixed(5), |row| {
            row.cell_with(GridCellSpec::new().vertical_align(VerticalAlign::Top), "x");
            row.cell_with(
                GridCellSpec::new().vertical_align(VerticalAlign::Center),
                "x",
            );
            row.cell_with(
                GridCellSpec::new().vertical_align(VerticalAlign::Bottom),
                "x",
            );
        });
    });
    let rects = child_rects(&view, 8);
    assert_eq!(rects[0].y, 0);
    assert_eq!(rects[1].y, 2);
    assert_eq!(rects[2].y, 4);
}

#[test]
fn column_span_area_includes_internal_gap() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::fixed(3), GridTrack::fixed(4)]);
        grid.column_gap(1);
        grid.row(|row| {
            row.cell_with(
                GridCellSpec::new().column_span(2),
                View::text("abcdefgh").fill_width().no_wrap(),
            );
        });
    });
    assert_eq!(child_rects(&view, 20)[0].width, 8);
}

#[test]
fn row_span_area_includes_internal_gap() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::flex()]);
        grid.row_gap(1);
        grid.row_with(GridTrack::fixed(2), |row| {
            row.cell_with(
                GridCellSpec::new().row_span(2),
                View::spacer(1).fill_width().fill_height(),
            );
        });
        grid.row_with(GridTrack::fixed(3), |_| {});
    })
    .fill_width();
    assert_eq!(child_rects(&view, 10)[0].height, 6);
}

#[test]
fn spanning_cell_grows_content_columns() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::content(), GridTrack::content()]);
        grid.row(|row| {
            row.cell_with(GridCellSpec::new().column_span(2), "abcdefghijkl");
        });
    });
    let size = measure_view(&view, 40);
    assert!(size.width >= 12);
    assert_eq!(child_rects(&view, 40)[0].width, size.width);
}

#[test]
fn spanning_cell_grows_content_rows() {
    let view = View::grid(|grid| {
        grid.row_gap(1);
        grid.row(|row| {
            row.cell_with(GridCellSpec::new().row_span(2), View::spacer(5));
        });
        grid.row(|_| {});
    });
    let size = measure_view(&view, 10);
    assert!(size.height >= 5);
}

#[test]
fn nested_grid_measures() {
    let inner = View::grid(|grid| {
        grid.columns([GridTrack::content(), GridTrack::content()]);
        grid.row(|row| {
            row.cell("ab");
            row.cell("cd");
        });
    });
    let outer = View::grid(|grid| {
        grid.row(|row| {
            row.cell(inner);
            row.cell("z");
        });
    });
    let block = compile_view(&outer, 20);
    assert!(block.rows[0].plain_text().contains("ab"));
    assert!(block.rows[0].plain_text().contains("cd"));
    assert!(block.rows[0].plain_text().contains("z"));
}

#[test]
fn grid_inside_row_and_row_inside_grid() {
    let grid_in_row = View::horizontal(|row| {
        row.child("L");
        row.flex(View::grid(|grid| {
            grid.columns([GridTrack::content(), GridTrack::flex()]);
            grid.row(|row| {
                row.cell("a");
                row.cell("b");
            });
        }));
    })
    .fill_width();
    assert!(
        compile_view(&grid_in_row, 20).rows[0]
            .plain_text()
            .contains("ab")
    );

    let row_in_grid = View::grid(|grid| {
        grid.columns([GridTrack::flex()]);
        grid.row(|row| {
            row.cell(View::horizontal(|row| {
                row.child("x");
                row.child("y");
            }));
        });
    })
    .fill_width();
    assert!(
        compile_view(&row_in_grid, 20).rows[0]
            .plain_text()
            .contains("xy")
    );
}

#[test]
fn padding_and_border_use_inner_width() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::flex()]);
        grid.row(|row| {
            row.cell(View::text("hello").fill_width());
        });
    })
    .fill_width()
    .padding(Insets::horizontal(2))
    .border(BorderSpec::plain());
    let laid_out = tree(&view, 20);
    let root = laid_out.node(laid_out.root);
    let child = laid_out.node(root.children[0]);
    assert!(child.rect.x >= 3);
    assert!(child.rect.width <= 20 - 6);
}

#[test]
fn bounds_apply_through_generic_measure() {
    let view = View::grid(|grid| {
        grid.columns([GridTrack::content()]);
        grid.row(|row| {
            row.cell("hello-world");
        });
    })
    .max_width(6);
    let size = measure_view(&view, 40);
    assert_eq!(size.width, 6);
}

#[test]
fn style_state_inherits_into_cells() {
    let theme = Theme::new().with_style_variant(
        "probe",
        StyleSelector::state("mode", "x"),
        StyleSpec::new().bold(),
    );
    let view = View::grid(|grid| {
        grid.row(|row| {
            row.cell(View::text("A").style(StyleRef::theme("probe")));
        });
    })
    .style_state("mode", "x");
    let compiler = ViewCompiler::new(&theme);
    let laid_out = compiler.layout_tree(&view, LayoutConstraints::width_only(4));
    let surface = ViewPainter.paint_tree(&compiler, &laid_out);
    assert!(surface.get(0, 0).style.bold);
}

#[test]
fn self_only_facts_do_not_enter_cells() {
    let theme = Theme::new().with_style_variant(
        "probe",
        StyleSelector::state("test.role", "heading"),
        StyleSpec::new().bold(),
    );
    let view = View::grid(|grid| {
        grid.row(|row| {
            row.cell(View::text("A").style(StyleRef::theme("probe")));
        });
    })
    .into_view()
    .style_fact("test.role", "heading");
    let compiler = ViewCompiler::new(&theme);
    let laid_out = compiler.layout_tree(&view, LayoutConstraints::width_only(4));
    let surface = ViewPainter.paint_tree(&compiler, &laid_out);
    assert!(!surface.get(0, 0).style.bold);
}

#[test]
fn row_span_fixed_then_content_absorbs_remainder() {
    let view = View::grid(|grid| {
        grid.row_gap(1);
        grid.row_with(GridTrack::fixed(1), |row| {
            row.cell_with(GridCellSpec::new().row_span(2), View::spacer(5));
        });
        grid.row(|_| {});
    });
    let rects = child_rects(&view, 10);
    assert_eq!(rects[0].height, 5);
    let size = measure_view(&view, 10);
    assert_eq!(size.height, 5);
}
