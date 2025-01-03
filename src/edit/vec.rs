//! Specialized editing in a table. Keeps a Vec of
//! the row-data.
//!
//! A widget that renders the table and can render
//! an edit-widget on top.
//!
//! __Examples__
//! For examples go to the rat-widget crate.
//! There is `examples/table_edit2.rs`.

use crate::edit::{Editor, EditorState, Mode};
use crate::rowselection::RowSelection;
use crate::textdata::Row;
use crate::{Table, TableContext, TableData, TableSelection, TableState};
use log::warn;
use rat_cursor::HasScreenCursor;
use rat_event::util::MouseFlags;
use rat_event::{ct_event, try_flow, HandleEvent, Outcome, Regular};
use rat_focus::{FocusBuilder, FocusFlag, HasFocus, Navigation};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::{StatefulWidget, Style};
use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

/// Extends TableData with the capability to set the actual data
/// at a later point in time.
///
/// This is needed to inject the data during rendering, while
/// leaving the rendering to the caller.
///
/// Due to life-time issues the data is given as Rc<>.
pub trait EditorData<D>: TableData<'static> {
    /// Set the actual table data.
    fn set_data(&mut self, data: Rc<RefCell<Vec<D>>>);
}

/// Widget that supports row-wise editing of a table.
///
/// This widget keeps a `Vec<RowData>` and modifies it.
///
/// It's parameterized with a `Editor` widget, that renders
/// the input line and handles events.
pub struct EditVec<'a, E>
where
    E: Editor + 'a,
{
    table: Table<'a, RowSelection>,
    table_data: Box<dyn EditorData<<<E as Editor>::State as EditorState>::Data>>,
    editor: E,
}

/// State for EditTable.
///
/// Contains `mode` to differentiate between edit/non-edit.
/// This will lock the focus to the input line while editing.
///
#[derive(Debug)]
pub struct EditVecState<S>
where
    S: EditorState,
{
    /// Editing mode.
    pub mode: Mode,

    /// Backing table.
    pub table: TableState<RowSelection>,
    /// Editor
    pub editor: S,
    /// Focus-flag for the whole editor widget.
    pub editor_focus: FocusFlag,
    /// Data store
    pub editor_data: Rc<RefCell<Vec<S::Data>>>,

    pub mouse: MouseFlags,
}

impl<'a, E> EditVec<'a, E>
where
    E: Editor + 'a,
{
    pub fn new(
        table_data: impl EditorData<<<E as Editor>::State as EditorState>::Data> + 'static,
        table: Table<'a, RowSelection>,
        editor: E,
    ) -> Self {
        Self {
            table,
            table_data: Box::new(table_data),
            editor,
        }
    }
}

impl<'a, D> TableData<'a> for Box<dyn EditorData<D> + 'a> {
    fn rows(&self) -> usize {
        (**self).rows()
    }

    fn header(&self) -> Option<Row<'a>> {
        (**self).header()
    }

    fn footer(&self) -> Option<Row<'a>> {
        (**self).footer()
    }

    fn row_height(&self, row: usize) -> u16 {
        (**self).row_height(row)
    }

    fn row_style(&self, row: usize) -> Option<Style> {
        (**self).row_style(row)
    }

    fn widths(&self) -> Vec<Constraint> {
        (**self).widths()
    }

    fn render_cell(
        &self,
        ctx: &TableContext,
        column: usize,
        row: usize,
        area: Rect,
        buf: &mut Buffer,
    ) {
        (**self).render_cell(ctx, column, row, area, buf)
    }
}

impl<'a, E> Debug for EditVec<'a, E>
where
    E: Debug,
    E: Editor + 'a,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditVec")
            .field("table", &self.table)
            .field("table_data", &"..dyn..")
            .field("editor", &self.editor)
            .finish()
    }
}

impl<'a, E> StatefulWidget for EditVec<'a, E>
where
    E: Editor + 'a,
{
    type State = EditVecState<E::State>;

    #[allow(clippy::collapsible_else_if)]
    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.table_data.set_data(state.editor_data.clone());
        self.table
            .data(self.table_data)
            .render(area, buf, &mut state.table);

        if state.mode == Mode::Insert || state.mode == Mode::Edit {
            if let Some(row) = state.table.selected() {
                // but it might be out of view
                if let Some((row_area, cell_areas)) = state.table.row_cells(row) {
                    self.editor
                        .render(row_area, &cell_areas, buf, &mut state.editor);
                }
            } else {
                if cfg!(debug_assertions) {
                    warn!("no row selection, not rendering editor");
                }
            }
        }
    }
}

impl<S> Default for EditVecState<S>
where
    S: Default,
    S: EditorState,
{
    fn default() -> Self {
        Self {
            mode: Mode::View,
            table: Default::default(),
            editor: S::default(),
            editor_focus: Default::default(),
            editor_data: Rc::new(RefCell::new(Vec::default())),
            mouse: Default::default(),
        }
    }
}

impl<S> HasFocus for EditVecState<S>
where
    S: EditorState,
{
    fn focus(&self) -> FocusFlag {
        match self.mode {
            Mode::View => self.table.focus(),
            Mode::Edit => self.editor_focus.clone(),
            Mode::Insert => self.editor_focus.clone(),
        }
    }

    fn area(&self) -> Rect {
        self.table.area()
    }

    fn navigable(&self) -> Navigation {
        match self.mode {
            Mode::View => self.table.navigable(),
            Mode::Edit | Mode::Insert => Navigation::Lock,
        }
    }

    fn is_focused(&self) -> bool {
        match self.mode {
            Mode::View => self.table.is_focused(),
            Mode::Edit | Mode::Insert => self.editor_focus.get(),
        }
    }

    fn lost_focus(&self) -> bool {
        match self.mode {
            Mode::View => self.table.is_focused(),
            Mode::Edit | Mode::Insert => self.editor_focus.lost(),
        }
    }

    fn gained_focus(&self) -> bool {
        match self.mode {
            Mode::View => self.table.is_focused(),
            Mode::Edit | Mode::Insert => self.editor_focus.gained(),
        }
    }
}

impl<S> HasScreenCursor for EditVecState<S>
where
    S: HasScreenCursor,
    S: EditorState,
{
    fn screen_cursor(&self) -> Option<(u16, u16)> {
        match self.mode {
            Mode::View => None,
            Mode::Edit | Mode::Insert => self.editor.screen_cursor(),
        }
    }
}

impl<S> EditVecState<S>
where
    S: EditorState,
{
    pub fn new(editor: S) -> Self {
        Self {
            mode: Mode::View,
            table: TableState::new(),
            editor,
            editor_focus: Default::default(),
            editor_data: Rc::new(RefCell::new(vec![])),
            mouse: Default::default(),
        }
    }

    pub fn named(name: &str, editor: S) -> Self {
        Self {
            mode: Mode::View,
            table: TableState::named(name),
            editor,
            editor_focus: Default::default(),
            editor_data: Rc::new(RefCell::new(vec![])),
            mouse: Default::default(),
        }
    }
}

impl<S> EditVecState<S>
where
    S: EditorState,
{
    /// Editing is active?
    pub fn is_editing(&self) -> bool {
        self.mode == Mode::Edit || self.mode == Mode::Insert
    }

    /// Is the current edit an insert?
    pub fn is_insert(&self) -> bool {
        self.mode == Mode::Insert
    }

    /// Remove the item at the selected row.
    pub fn remove(&mut self, row: usize) {
        if self.mode != Mode::View {
            return;
        }
        if row < self.editor_data.borrow().len() {
            self.editor_data.borrow_mut().remove(row);
            self.table.items_removed(row, 1);
            if !self.table.scroll_to_row(row) {
                self.table.scroll_to_row(row.saturating_sub(1));
            }
        }
    }

    /// Edit a new item inserted at the selected row.
    pub fn edit_new(&mut self, row: usize, ctx: &S::Context<'_>) -> Result<(), S::Err> {
        if self.mode != Mode::View {
            return Ok(());
        }
        let value = self.editor.new_edit_data(ctx)?;
        self.editor.set_edit_data(&value, ctx)?;
        self.editor_data.borrow_mut().insert(row, value);
        self._start(row, Mode::Insert);
        Ok(())
    }

    /// Edit the item at the selected row.
    pub fn edit(&mut self, row: usize, ctx: &S::Context<'_>) -> Result<(), S::Err> {
        if self.mode != Mode::View {
            return Ok(());
        }
        {
            let value = &self.editor_data.borrow()[row];
            self.editor.set_edit_data(value, ctx)?;
        }
        self._start(row, Mode::Edit);
        Ok(())
    }

    fn _start(&mut self, pos: usize, mode: Mode) {
        if self.table.is_focused() {
            self.table.focus().set(false);
            self.editor_focus.set(true);
            FocusBuilder::for_container(&self.editor).first();
        }

        self.mode = mode;
        if self.mode == Mode::Insert {
            self.table.items_added(pos, 1);
        }
        self.table.move_to(pos);
        self.table.scroll_to_col(0);
    }

    /// Cancel editing.
    ///
    /// Updates the state to remove the edited row.
    pub fn cancel(&mut self) {
        if self.mode == Mode::View {
            return;
        }
        let Some(row) = self.table.selected() else {
            return;
        };
        if self.mode == Mode::Insert {
            self.editor_data.borrow_mut().remove(row);
            self.table.items_removed(row, 1);
        }
        self._stop();
    }

    /// Commit the changes in the editor.
    pub fn commit(&mut self, ctx: &S::Context<'_>) -> Result<(), S::Err> {
        if self.mode == Mode::View {
            return Ok(());
        }
        let Some(row) = self.table.selected() else {
            return Ok(());
        };
        {
            let value = &mut self.editor_data.borrow_mut()[row];
            self.editor.get_edit_data(value, ctx)?;
        }
        self._stop();
        Ok(())
    }

    pub fn commit_and_append(&mut self, ctx: &S::Context<'_>) -> Result<(), S::Err> {
        self.commit(ctx)?;
        if let Some(row) = self.table.selected() {
            self.edit_new(row + 1, ctx)?;
        }
        Ok(())
    }

    pub fn commit_and_edit(&mut self, ctx: &S::Context<'_>) -> Result<(), S::Err> {
        let Some(row) = self.table.selected() else {
            return Ok(());
        };

        self.commit(ctx)?;
        self.table.select(Some(row + 1));
        self.edit(row + 1, ctx)?;
        Ok(())
    }

    fn _stop(&mut self) {
        self.mode = Mode::View;
        if self.editor_focus.get() {
            self.table.focus.set(true);
            self.editor_focus.set(false);
        }
        self.table.scroll_to_col(0);
    }
}

impl<'a, S> HandleEvent<crossterm::event::Event, &'a S::Context<'a>, Result<Outcome, S::Err>>
    for EditVecState<S>
where
    S: HandleEvent<crossterm::event::Event, Regular, Outcome>,
    S: EditorState,
{
    fn handle(
        &mut self,
        event: &crossterm::event::Event,
        ctx: &'a S::Context<'a>,
    ) -> Result<Outcome, S::Err> {
        if self.mode == Mode::Edit || self.mode == Mode::Insert {
            try_flow!(match self.editor.handle(event, Regular) {
                Outcome::Continue => Outcome::Continue,
                Outcome::Unchanged => Outcome::Unchanged,
                r => {
                    if let Some(col) = self.editor.focused_col() {
                        self.table.scroll_to_col(col);
                    }
                    r
                }
            });

            try_flow!(match event {
                ct_event!(keycode press Esc) => {
                    self.cancel();
                    Outcome::Changed
                }
                ct_event!(keycode press Enter) => {
                    if self.table.selected() < Some(self.table.rows().saturating_sub(1)) {
                        self.commit_and_edit(ctx)?;
                        Outcome::Changed
                    } else {
                        self.commit_and_append(ctx)?;
                        Outcome::Changed
                    }
                }
                ct_event!(keycode press Up) => {
                    self.commit(ctx)?;
                    Outcome::Changed
                }
                ct_event!(keycode press Down) => {
                    self.commit(ctx)?;
                    Outcome::Changed
                }
                _ => Outcome::Continue,
            });

            Ok(Outcome::Continue)
        } else {
            try_flow!(match event {
                ct_event!(mouse any for m) if self.mouse.doubleclick(self.table.table_area, m) => {
                    if let Some((_col, row)) = self.table.cell_at_clicked((m.column, m.row)) {
                        self.edit(row, ctx)?;
                        Outcome::Changed
                    } else {
                        Outcome::Continue
                    }
                }
                _ => Outcome::Continue,
            });

            try_flow!(match event {
                ct_event!(keycode press Insert) => {
                    if let Some(row) = self.table.selected() {
                        self.edit_new(row, ctx)?;
                    }
                    Outcome::Changed
                }
                ct_event!(keycode press Delete) => {
                    if let Some(row) = self.table.selected() {
                        self.remove(row);
                    }
                    Outcome::Changed
                }
                ct_event!(keycode press Enter) | ct_event!(keycode press F(2)) => {
                    if let Some(row) = self.table.selected() {
                        self.edit(row, ctx)?;
                    }
                    Outcome::Changed
                }
                ct_event!(keycode press Down) => {
                    if let Some((_column, row)) = self.table.selection.lead_selection() {
                        if row == self.table.rows().saturating_sub(1) {
                            self.edit_new(row + 1, ctx)?;
                            Outcome::Changed
                        } else {
                            Outcome::Continue
                        }
                    } else {
                        Outcome::Continue
                    }
                }
                _ => {
                    Outcome::Continue
                }
            });

            try_flow!(self.table.handle(event, Regular));

            Ok(Outcome::Continue)
        }
    }
}
