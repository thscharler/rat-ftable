//!
//! This widget helps with row-wise editing of table-data.
//!
//! todo: example is missing. this is hard to grasp.
//!

use crate::event::EditOutcome;
use crate::rowselection::RowSelection;
use crate::{FTable, TableSelection};
use rat_event::util::MouseFlags;
use rat_event::{ct_event, flow, FocusKeys, HandleEvent, MouseOnly, Outcome};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::StatefulWidget;

/// Widget that supports row-wise editing of a table.
///
/// It's parameterized with a `Editor` widget, that renders
/// the input line.
#[derive(Debug)]
pub struct FEditTable<'a, Editor: EditorWidget + 'a> {
    table: FTable<'a, RowSelection>,
    edit: Editor,
}

/// Edit state for the table.
///
/// If the edit-state is set, this widget switches to edit-mode.
#[derive(Debug, Default)]
pub struct FEditTableState<EditorState> {
    /// Backing table.
    pub table: crate::FTableState<RowSelection>,
    /// Editor state.
    pub edit: Option<EditorState>,

    pub mouse: MouseFlags,
}

/// StatefulWidget alike trait.
///
/// This one takes a slice of areas for all the cells in the table.
pub trait EditorWidget {
    /// State associated with the stateful widget.
    type State;

    /// Standard render call, but with added areas for each cell.
    fn render(self, area: Rect, cell_areas: &[Rect], buf: &mut Buffer, state: &mut Self::State);
}

impl<'a, Editor> FEditTable<'a, Editor>
where
    Editor: EditorWidget + 'a,
{
    pub fn new(table: FTable<'a, RowSelection>, edit: Editor) -> Self {
        Self { table, edit }
    }
}

impl<'a, Editor> StatefulWidget for FEditTable<'a, Editor>
where
    Editor: EditorWidget + 'a,
{
    type State = FEditTableState<Editor::State>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.table.render(area, buf, &mut state.table);

        if let Some(edit_state) = &mut state.edit {
            // expect a selected row
            if let Some(row) = state.table.selected() {
                // but it might be out of view
                if let Some((row_area, cell_areas)) = state.table.row_cells(row) {
                    self.edit.render(row_area, &cell_areas, buf, edit_state);
                }
            }
        }
    }
}

impl<EState, EQualifier> HandleEvent<crossterm::event::Event, EQualifier, EditOutcome>
    for FEditTableState<EState>
where
    EState: HandleEvent<crossterm::event::Event, EQualifier, EditOutcome>,
{
    fn handle(&mut self, event: &crossterm::event::Event, qualifier: EQualifier) -> EditOutcome {
        flow!(match event {
            ct_event!(mouse any for m) if self.mouse.doubleclick(self.table.table_area, m) => {
                if self
                    .table
                    .cell_at_clicked((m.column, m.row).into())
                    .is_some()
                {
                    EditOutcome::Edit
                } else {
                    EditOutcome::NotUsed
                }
            }
            _ => EditOutcome::NotUsed,
        });

        if let Some(edit_state) = self.edit.as_mut() {
            flow!(edit_state.handle(event, qualifier));

            flow!(match event {
                ct_event!(keycode press Esc) => {
                    EditOutcome::Cancel
                }
                ct_event!(keycode press Enter) | ct_event!(keycode press Up) => {
                    EditOutcome::Commit
                }
                ct_event!(keycode press Down) => {
                    if self.table.selected() != Some(self.table.rows().saturating_sub(1)) {
                        EditOutcome::Commit
                    } else {
                        EditOutcome::NotUsed
                    }
                }
                _ => EditOutcome::NotUsed,
            });

            EditOutcome::NotUsed
        } else {
            flow!(match event {
                ct_event!(keycode press Insert) => {
                    EditOutcome::Insert
                }
                ct_event!(keycode press Delete) => {
                    EditOutcome::Remove
                }
                ct_event!(keycode press Enter) | ct_event!(keycode press F(2)) => {
                    EditOutcome::Edit
                }
                ct_event!(keycode press Down) => 'f: {
                    if let Some((_column, row)) = self.table.selection.lead_selection() {
                        if row == self.table.rows().saturating_sub(1) {
                            break 'f EditOutcome::Append;
                        }
                    }
                    EditOutcome::NotUsed
                }
                _ => {
                    EditOutcome::NotUsed
                }
            });

            match self.table.handle(event, FocusKeys) {
                Outcome::NotUsed => EditOutcome::NotUsed,
                Outcome::Unchanged => EditOutcome::Unchanged,
                Outcome::Changed => EditOutcome::Changed,
            }
        }
    }
}

/// Handle extended edit-events.
///
/// Table events are only handled if focus is true.
/// Mouse events are processed if they are in range.
///
/// The qualifier indicates which event-handler for EState will
/// be called. Or it can be used to pass in some context.
pub fn handle_edit_events<EState, EQualifier>(
    state: &mut FEditTableState<EState>,
    focus: bool,
    event: &crossterm::event::Event,
    qualifier: EQualifier,
) -> EditOutcome
where
    EState: HandleEvent<crossterm::event::Event, EQualifier, EditOutcome>,
{
    if focus {
        state.handle(event, qualifier)
    } else {
        let r = state.table.handle(event, MouseOnly);
        r.into()
    }
}