#![allow(unreachable_pub)]

use crate::mini_salsa::layout_grid;
use rat_ftable::selection::RowSelection;
use rat_ftable::TableState;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Widget;
use std::fmt::Debug;

pub fn render_tablestate<Selection: Debug>(
    state: &TableState<Selection>,
    area: Rect,
    buf: &mut Buffer,
) {
    let l_grid = layout_grid::<2, 12>(
        area,
        Layout::horizontal([Constraint::Length(15), Constraint::Length(20)]),
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ]),
    );

    "count_rows".render(l_grid[0][0], buf);
    format!("{}", state._counted_rows)
        .to_string()
        .render(l_grid[1][0], buf);

    "rows".render(l_grid[0][1], buf);
    format!("{}", state.rows)
        .to_string()
        .render(l_grid[1][1], buf);

    "row_offset".render(l_grid[0][2], buf);
    format!("{}", state.vscroll.offset())
        .to_string()
        .render(l_grid[1][2], buf);

    "max_row_offset".render(l_grid[0][3], buf);
    format!("{}", state.vscroll.max_offset())
        .to_string()
        .render(l_grid[1][3], buf);

    "row_page_len".render(l_grid[0][4], buf);
    format!("{}", state.vscroll.page_len())
        .to_string()
        .render(l_grid[1][4], buf);

    "row_scroll".render(l_grid[0][5], buf);
    format!("{}", state.vscroll.scroll_by())
        .to_string()
        .render(l_grid[1][5], buf);

    "columns".render(l_grid[0][6], buf);
    format!("{}", state.columns)
        .to_string()
        .render(l_grid[1][6], buf);

    "col_offset".render(l_grid[0][7], buf);
    format!("{}", state.hscroll.offset())
        .to_string()
        .render(l_grid[1][7], buf);

    "max_col_offset".render(l_grid[0][8], buf);
    format!("{}", state.hscroll.max_offset())
        .to_string()
        .render(l_grid[1][8], buf);

    "col_page_len".render(l_grid[0][9], buf);
    format!("{}", state.hscroll.page_len())
        .to_string()
        .render(l_grid[1][9], buf);

    "col_scrollby".render(l_grid[0][10], buf);
    format!("{}", state.hscroll.scroll_by())
        .to_string()
        .render(l_grid[1][10], buf);
}

pub fn render_tablestate_row(state: &TableState<RowSelection>, area: Rect, buf: &mut Buffer) {
    let l_grid = layout_grid::<2, 12>(
        area,
        Layout::horizontal([Constraint::Length(15), Constraint::Length(20)]),
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ]),
    );

    "count_rows".render(l_grid[0][0], buf);
    format!("{}", state._counted_rows)
        .to_string()
        .render(l_grid[1][0], buf);

    "rows".render(l_grid[0][1], buf);
    format!("{}", state.rows)
        .to_string()
        .render(l_grid[1][1], buf);

    "row_offset".render(l_grid[0][2], buf);
    format!("{}", state.vscroll.offset())
        .to_string()
        .render(l_grid[1][2], buf);

    "max_row_offset".render(l_grid[0][3], buf);
    format!("{}", state.vscroll.max_offset())
        .to_string()
        .render(l_grid[1][3], buf);

    "row_page_len".render(l_grid[0][4], buf);
    format!("{}", state.vscroll.page_len())
        .to_string()
        .render(l_grid[1][4], buf);

    "row_scroll".render(l_grid[0][5], buf);
    format!("{}", state.vscroll.scroll_by())
        .to_string()
        .render(l_grid[1][5], buf);

    "columns".render(l_grid[0][6], buf);
    format!("{}", state.columns)
        .to_string()
        .render(l_grid[1][6], buf);

    "col_offset".render(l_grid[0][7], buf);
    format!("{}", state.hscroll.offset())
        .to_string()
        .render(l_grid[1][7], buf);

    "max_col_offset".render(l_grid[0][8], buf);
    format!("{}", state.hscroll.max_offset())
        .to_string()
        .render(l_grid[1][8], buf);

    "col_page_len".render(l_grid[0][9], buf);
    format!("{}", state.hscroll.page_len())
        .to_string()
        .render(l_grid[1][9], buf);

    "col_scrollby".render(l_grid[0][10], buf);
    format!("{}", state.hscroll.scroll_by())
        .to_string()
        .render(l_grid[1][10], buf);

    "selection".render(l_grid[0][11], buf);
    format!("{:?}", state.selection.lead_row)
        .to_string()
        .render(l_grid[1][11], buf);
}
