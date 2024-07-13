#![allow(clippy::collapsible_if)]

use crate::_private::NonExhaustive;
use crate::event::{DoubleClick, DoubleClickOutcome, EditKeys, EditOutcome};
use crate::selection::{CellSelection, RowSelection, RowSetSelection};
use crate::table::data::{DataRepr, DataReprIter};
use crate::textdata::{Row, TextTableData};
use crate::util::{revert_style, transfer_buffer};
use crate::{RTableContext, TableData, TableDataIter, TableSelection};
#[allow(unused_imports)]
use log::debug;
#[cfg(debug_assertions)]
use log::warn;
use rat_event::util::MouseFlags;
use rat_event::{ct_event, HandleEvent, MouseOnly, Outcome, Regular};
use rat_focus::{FocusFlag, HasFocusFlag};
use rat_scrolled::{layout_scroll, Scroll, ScrollState};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
#[cfg(debug_assertions)]
use ratatui::style::Stylize;
#[cfg(debug_assertions)]
use ratatui::text::Text;
use ratatui::widgets::{Block, StatefulWidget, StatefulWidgetRef, Widget, WidgetRef};
use std::cmp::{max, min};
use std::collections::HashSet;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

/// Table widget.
///
/// Can be used like a ratatui::Table, but the benefits only
/// show if you use [Table::data] or [Table::iter] to set the table data.
///
/// See [Table::data] and [Table::iter] for an example.
#[derive(Debug, Default)]
pub struct Table<'a, Selection> {
    data: DataRepr<'a>,
    no_row_count: bool,

    header: Option<Row<'a>>,
    footer: Option<Row<'a>>,

    widths: Vec<Constraint>,
    flex: Flex,
    column_spacing: u16,
    layout_width: Option<u16>,
    auto_layout_width: bool,

    block: Option<Block<'a>>,
    hscroll: Option<Scroll<'a>>,
    vscroll: Option<Scroll<'a>>,

    header_style: Option<Style>,
    footer_style: Option<Style>,
    style: Style,

    select_row_style: Option<Style>,
    show_row_focus: bool,
    select_column_style: Option<Style>,
    show_column_focus: bool,
    select_cell_style: Option<Style>,
    show_cell_focus: bool,
    select_header_style: Option<Style>,
    show_header_focus: bool,
    select_footer_style: Option<Style>,
    show_footer_focus: bool,

    focus_style: Option<Style>,

    debug: bool,

    _phantom: PhantomData<Selection>,
}

mod data {
    use crate::textdata::TextTableData;
    use crate::{RTableContext, TableData, TableDataIter};
    #[allow(unused_imports)]
    use log::debug;
    #[allow(unused_imports)]
    use log::warn;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Style, Stylize};
    use std::fmt::{Debug, Formatter};

    #[derive(Default)]
    pub(super) enum DataRepr<'a> {
        #[default]
        None,
        Text(TextTableData<'a>),
        Data(Box<dyn TableData<'a> + 'a>),
        Iter(Box<dyn TableDataIter<'a> + 'a>),
    }

    impl<'a> DataRepr<'a> {
        pub(super) fn into_iter(self) -> DataReprIter<'a, 'a> {
            match self {
                DataRepr::None => DataReprIter::None,
                DataRepr::Text(v) => DataReprIter::IterText(v, None),
                DataRepr::Data(v) => DataReprIter::IterData(v, None),
                DataRepr::Iter(v) => DataReprIter::IterIter(v),
            }
        }

        pub(super) fn iter<'b>(&'b self) -> DataReprIter<'a, 'b> {
            match self {
                DataRepr::None => DataReprIter::None,
                DataRepr::Text(v) => DataReprIter::IterDataRef(v, None),
                DataRepr::Data(v) => DataReprIter::IterDataRef(v.as_ref(), None),
                DataRepr::Iter(v) => {
                    // TableDataIter might not implement a valid cloned().
                    if let Some(v) = v.cloned() {
                        DataReprIter::IterIter(v)
                    } else {
                        DataReprIter::Invalid(None)
                    }
                }
            }
        }
    }

    impl<'a> Debug for DataRepr<'a> {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Data").finish()
        }
    }

    #[derive(Default)]
    pub(super) enum DataReprIter<'a, 'b> {
        #[default]
        None,
        Invalid(Option<usize>),
        IterText(TextTableData<'a>, Option<usize>),
        IterData(Box<dyn TableData<'a> + 'a>, Option<usize>),
        IterDataRef(&'b dyn TableData<'a>, Option<usize>),
        IterIter(Box<dyn TableDataIter<'a> + 'a>),
    }

    impl<'a, 'b> TableDataIter<'a> for DataReprIter<'a, 'b> {
        fn rows(&self) -> Option<usize> {
            match self {
                DataReprIter::None => Some(0),
                DataReprIter::Invalid(_) => Some(1),
                DataReprIter::IterText(v, _) => Some(v.rows.len()),
                DataReprIter::IterData(v, _) => Some(v.rows()),
                DataReprIter::IterDataRef(v, _) => Some(v.rows()),
                DataReprIter::IterIter(v) => v.rows(),
            }
        }

        fn nth(&mut self, n: usize) -> bool {
            let incr = |row: &mut Option<usize>, rows: usize| match *row {
                None => {
                    *row = Some(n);
                    n < rows
                }
                Some(w) => {
                    *row = Some(w + n + 1);
                    w + n + 1 < rows
                }
            };

            match self {
                DataReprIter::None => false,
                DataReprIter::Invalid(row) => incr(row, 1),
                DataReprIter::IterText(v, row) => incr(row, v.rows.len()),
                DataReprIter::IterData(v, row) => incr(row, v.rows()),
                DataReprIter::IterDataRef(v, row) => incr(row, v.rows()),
                DataReprIter::IterIter(v) => v.nth(n),
            }
        }

        /// Row height.
        fn row_height(&self) -> u16 {
            match self {
                DataReprIter::None => 1,
                DataReprIter::Invalid(_) => 1,
                DataReprIter::IterText(v, n) => v.row_height(n.expect("row")),
                DataReprIter::IterData(v, n) => v.row_height(n.expect("row")),
                DataReprIter::IterDataRef(v, n) => v.row_height(n.expect("row")),
                DataReprIter::IterIter(v) => v.row_height(),
            }
        }

        fn row_style(&self) -> Option<Style> {
            match self {
                DataReprIter::None => None,
                DataReprIter::Invalid(_) => Some(Style::new().white().on_red()),
                DataReprIter::IterText(v, n) => v.row_style(n.expect("row")),
                DataReprIter::IterData(v, n) => v.row_style(n.expect("row")),
                DataReprIter::IterDataRef(v, n) => v.row_style(n.expect("row")),
                DataReprIter::IterIter(v) => v.row_style(),
            }
        }

        /// Render the cell given by column/row.
        fn render_cell(&self, ctx: &RTableContext, column: usize, area: Rect, buf: &mut Buffer) {
            match self {
                DataReprIter::None => {}
                DataReprIter::Invalid(_) => {
                    if column == 0 {
                        #[cfg(debug_assertions)]
                        warn!("Table::render_ref - TableDataIter must implement a valid cloned() for this to work.");

                        buf.set_string(
                            area.x,
                            area.y,
                            "TableDataIter must implement a valid cloned() for this",
                            Style::default(),
                        );
                    }
                }
                DataReprIter::IterText(v, n) => {
                    v.render_cell(ctx, column, n.expect("row"), area, buf)
                }
                DataReprIter::IterData(v, n) => {
                    v.render_cell(ctx, column, n.expect("row"), area, buf)
                }
                DataReprIter::IterDataRef(v, n) => {
                    v.render_cell(ctx, column, n.expect("row"), area, buf)
                }
                DataReprIter::IterIter(v) => v.render_cell(ctx, column, area, buf),
            }
        }
    }
}

/// Combined style.
#[derive(Debug)]
pub struct TableStyle {
    pub style: Style,
    pub header_style: Option<Style>,
    pub footer_style: Option<Style>,

    pub select_row_style: Option<Style>,
    pub select_column_style: Option<Style>,
    pub select_cell_style: Option<Style>,
    pub select_header_style: Option<Style>,
    pub select_footer_style: Option<Style>,

    pub show_row_focus: bool,
    pub show_column_focus: bool,
    pub show_cell_focus: bool,
    pub show_header_focus: bool,
    pub show_footer_focus: bool,

    pub focus_style: Option<Style>,

    pub non_exhaustive: NonExhaustive,
}

/// Table state.
#[derive(Debug, Clone)]
pub struct TableState<Selection> {
    /// Current focus state.
    pub focus: FocusFlag,

    /// Total area.
    pub area: Rect,
    /// Area inside the border and scrollbars
    pub inner: Rect,

    /// Total header area.
    pub header_area: Rect,
    /// Total table area.
    pub table_area: Rect,
    /// Area per visible row. The first element is at row_offset.
    pub row_areas: Vec<Rect>,
    /// Area for each column plus the following spacer if any.
    /// Invisible columns have width 0, height is the height of the table_area.
    pub column_areas: Vec<Rect>,
    /// Layout areas for each column plus the following spacer if any.
    /// Positions are 0-based, y and height are 0.
    pub column_layout: Vec<Rect>,
    /// Total footer area.
    pub footer_area: Rect,

    /// Row count.
    pub rows: usize,
    // debug info
    pub _counted_rows: usize,
    /// Column count.
    pub columns: usize,

    /// Row scrolling data.
    pub vscroll: ScrollState,
    /// Column scrolling data.
    pub hscroll: ScrollState,

    /// Selection data.
    pub selection: Selection,

    /// Helper for mouse interactions.
    pub mouse: MouseFlags,

    pub non_exhaustive: NonExhaustive,
}

impl<'a, Selection> Table<'a, Selection> {
    /// New, empty Table.
    pub fn new() -> Self
    where
        Selection: Default,
    {
        Self::default()
    }

    /// Create a new Table with preformatted data. For compatibility
    /// with ratatui.
    ///
    /// Use of [Table::data] is preferred.
    pub fn new_ratatui<R, C>(rows: R, widths: C) -> Self
    where
        R: IntoIterator,
        R::Item: Into<Row<'a>>,
        C: IntoIterator,
        C::Item: Into<Constraint>,
        Selection: Default,
    {
        let widths = widths.into_iter().map(|v| v.into()).collect::<Vec<_>>();
        let data = TextTableData {
            rows: rows.into_iter().map(|v| v.into()).collect(),
        };
        Self {
            data: DataRepr::Text(data),
            widths,
            ..Default::default()
        }
    }

    /// Set preformatted row-data. For compatibility with ratatui.
    ///
    /// Use of [Table::data] is preferred.
    pub fn rows<T>(mut self, rows: T) -> Self
    where
        T: IntoIterator<Item = Row<'a>>,
    {
        let rows = rows.into_iter().collect();
        self.data = DataRepr::Text(TextTableData { rows });
        self
    }

    /// Set a reference to the TableData facade to your data.
    ///
    /// The way to go is to define a small struct that contains just a
    /// reference to your data. Then implement TableData for this struct.
    ///
    /// ```rust
    /// use ratatui::buffer::Buffer;
    /// use ratatui::layout::Rect;
    /// use ratatui::prelude::Style;
    /// use ratatui::text::Span;
    /// use ratatui::widgets::{StatefulWidget, Widget};
    /// use rat_ftable::{Table, RTableContext, TableState, TableData};
    ///
    /// # struct SampleRow;
    /// # let area = Rect::default();
    /// # let mut buf = Buffer::empty(area);
    /// # let buf = &mut buf;
    ///
    /// struct Data1<'a>(&'a [SampleRow]);
    ///
    /// impl<'a> TableData<'a> for Data1<'a> {
    ///     fn rows(&self) -> usize {
    ///         self.0.len()
    ///     }
    ///
    ///     fn row_height(&self, row: usize) -> u16 {
    ///         // to some calculations ...
    ///         1
    ///     }
    ///
    ///     fn row_style(&self, row: usize) -> Style {
    ///         // to some calculations ...
    ///         Style::default()
    ///     }
    ///
    ///     fn render_cell(&self, ctx: &RTableContext, column: usize, row: usize, area: Rect, buf: &mut Buffer) {
    ///         if let Some(data) = self.0.get(row) {
    ///             let rend = match column {
    ///                 0 => Span::from("column1"),
    ///                 1 => Span::from("column2"),
    ///                 2 => Span::from("column3"),
    ///                 _ => return
    ///             };
    ///             rend.render(area, buf);
    ///         }
    ///     }
    /// }
    ///
    /// // When you are creating the table widget you hand over a reference
    /// // to the facade struct.
    ///
    /// let my_data_somewhere_else = vec![SampleRow;999999];
    /// let mut table_state_somewhere_else = TableState::default();
    ///
    /// // ...
    ///
    /// let table1 = Table::default().data(Data1(&my_data_somewhere_else));
    /// table1.render(area, buf, &mut table_state_somewhere_else);
    /// ```
    #[inline]
    pub fn data(mut self, data: impl TableData<'a> + 'a) -> Self {
        self.widths = data.widths();
        self.header = data.header();
        self.footer = data.footer();
        self.data = DataRepr::Data(Box::new(data));
        self
    }

    ///
    /// Alternative representation for the data as a kind of Iterator.
    /// It uses interior iteration, which fits quite nice for this and
    /// avoids handing out lifetime bound results of the actual iterator.
    /// Which is a bit nightmarish to get right.
    ///
    ///
    /// Caution: If you can't give the number of rows, the table will iterate over all
    /// the data. See [Table::no_row_count].
    ///
    /// ```rust
    /// use std::iter::{Enumerate};
    /// use std::slice::Iter;
    /// use format_num_pattern::NumberFormat;
    /// use ratatui::buffer::Buffer;
    /// use ratatui::layout::{Constraint, Rect};
    /// use ratatui::prelude::Color;
    /// use ratatui::style::{Style, Stylize};
    /// use ratatui::text::Span;
    /// use ratatui::widgets::{Widget, StatefulWidget};
    /// use rat_ftable::{Table, RTableContext, TableState, TableDataIter};
    ///
    /// # struct Data {
    /// #     table_data: Vec<Sample>
    /// # }
    /// #
    /// # struct Sample {
    /// #     pub text: String
    /// # }
    /// #
    /// # let data = Data {
    /// #     table_data: vec![],
    /// # };
    /// # let area = Rect::default();
    /// # let mut buf = Buffer::empty(area);
    /// # let buf = &mut buf;
    ///
    /// struct RowIter1<'a> {
    ///     iter: Enumerate<Iter<'a, Sample>>,
    ///     item: Option<(usize, &'a Sample)>,
    /// }
    ///
    /// impl<'a> TableDataIter<'a> for RowIter1<'a> {
    ///     fn rows(&self) -> Option<usize> {
    ///         // If you can, give the length. Otherwise,
    ///         // the table will iterate all to find out a length.
    ///         None
    ///         // Some(100_000)
    ///     }
    ///
    ///     /// Select the nth element from the current position.
    ///     fn nth(&mut self, n: usize) -> bool {
    ///         self.item = self.iter.nth(n);
    ///         self.item.is_some()
    ///     }
    ///
    ///     /// Row height.
    ///     fn row_height(&self) -> u16 {
    ///         1
    ///     }
    ///
    ///     /// Row style.
    ///     fn row_style(&self) -> Style {
    ///         Style::default()
    ///     }
    ///
    ///     /// Render one cell.
    ///     fn render_cell(&self,
    ///                     ctx: &RTableContext,
    ///                     column: usize,
    ///                     area: Rect,
    ///                     buf: &mut Buffer)
    ///     {
    ///         let row = self.item.expect("data");
    ///         match column {
    ///             0 => {
    ///                 let row_fmt = NumberFormat::new("000000").expect("fmt");
    ///                 let span = Span::from(row_fmt.fmt_u(row.0));
    ///                 buf.set_style(area, Style::new().black().bg(Color::from_u32(0xe7c787)));
    ///                 span.render(area, buf);
    ///             }
    ///             1 => {
    ///                 let span = Span::from(&row.1.text);
    ///                 span.render(area, buf);
    ///             }
    ///             _ => {}
    ///         }
    ///     }
    /// }
    ///
    /// let mut rit = RowIter1 {
    ///     iter: data.table_data.iter().enumerate(),
    ///     item: None,
    /// };
    ///
    /// let table1 = Table::default()
    ///     .iter(&mut rit)
    ///     .widths([
    ///         Constraint::Length(6),
    ///         Constraint::Length(20)
    ///     ]);
    ///
    /// let mut table_state_somewhere_else = TableState::default();
    ///
    /// table1.render(area, buf, &mut table_state_somewhere_else);
    /// ```
    ///
    #[inline]
    pub fn iter(mut self, data: impl TableDataIter<'a> + 'a) -> Self {
        #[cfg(debug_assertions)]
        if data.rows().is_none() {
            warn!("Table::iter - rows is None, this will be slower");
        }
        self.header = data.header();
        self.footer = data.footer();
        self.widths = data.widths();
        self.data = DataRepr::Iter(Box::new(data));
        self
    }

    /// If you work with an TableDataIter to fill the table, and
    /// if you don't return a count with rows(), Table will run
    /// through all your iterator to find the actual number of rows.
    ///
    /// This may take its time.
    ///
    /// If you set no_row_count(true), this part will be skipped, and
    /// the row count will be set to an estimate of usize::MAX.
    /// This will destroy your ability to jump to the end of the data,
    /// but otherwise it's fine.
    /// You can still page-down through the data, and if you ever
    /// reach the end, the correct row-count can be established.
    ///
    /// _Extra info_: This might be only useful if you have a LOT of data.
    /// In my test it changed from 1.5ms to 150µs for about 100.000 rows.
    /// And 1.5ms is still not that much ... so you probably want to
    /// test without this first and then decide.
    pub fn no_row_count(mut self, no_row_count: bool) -> Self {
        self.no_row_count = no_row_count;
        self
    }

    /// Set the table-header.
    #[inline]
    pub fn header(mut self, header: Row<'a>) -> Self {
        self.header = Some(header);
        self
    }

    /// Set the table-footer.
    #[inline]
    pub fn footer(mut self, footer: Row<'a>) -> Self {
        self.footer = Some(footer);
        self
    }

    /// Column widths as Constraints.
    pub fn widths<I>(mut self, widths: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Constraint>,
    {
        self.widths = widths.into_iter().map(|v| v.into()).collect();
        self
    }

    /// Flex for layout.
    #[inline]
    pub fn flex(mut self, flex: Flex) -> Self {
        self.flex = flex;
        self
    }

    /// Spacing between columns.
    #[inline]
    pub fn column_spacing(mut self, spacing: u16) -> Self {
        self.column_spacing = spacing;
        self
    }

    /// Overrides the width of the rendering area for layout purposes.
    /// Layout uses this width, even if it means that some columns are
    /// not visible.
    #[inline]
    pub fn layout_width(mut self, width: u16) -> Self {
        self.layout_width = Some(width);
        self
    }

    /// Calculates the width from the given column-constraints.
    /// If a fixed layout_width() is set too, that one will win.
    ///
    /// Panic:
    /// Rendering will panic, if any constraint other than Constraint::Length(),
    /// Constraint::Min() or Constraint::Max() is used.
    #[inline]
    pub fn auto_layout_width(mut self, auto: bool) -> Self {
        self.auto_layout_width = auto;
        self
    }

    /// Draws a block around the table widget.
    #[inline]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Scrollbars
    pub fn scroll(mut self, scroll: Scroll<'a>) -> Self {
        self.hscroll = Some(scroll.clone().override_horizontal());
        self.vscroll = Some(scroll.override_vertical());
        self
    }

    /// Scrollbars
    pub fn hscroll(mut self, scroll: Scroll<'a>) -> Self {
        self.hscroll = Some(scroll.override_horizontal());
        self
    }

    /// Scrollbars
    pub fn vscroll(mut self, scroll: Scroll<'a>) -> Self {
        self.vscroll = Some(scroll.override_vertical());
        self
    }

    /// Set all styles as a bundle.
    #[inline]
    pub fn styles(mut self, styles: TableStyle) -> Self {
        self.style = styles.style;
        self.header_style = styles.header_style;
        self.footer_style = styles.footer_style;

        self.select_row_style = styles.select_row_style;
        self.show_row_focus = styles.show_row_focus;
        self.select_column_style = styles.select_column_style;
        self.show_column_focus = styles.show_column_focus;
        self.select_cell_style = styles.select_cell_style;
        self.show_cell_focus = styles.show_cell_focus;
        self.select_header_style = styles.select_header_style;
        self.show_header_focus = styles.show_header_focus;
        self.select_footer_style = styles.select_footer_style;
        self.show_footer_focus = styles.show_footer_focus;

        self.focus_style = styles.focus_style;
        self
    }

    /// Base style for the table.
    #[inline]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Base style for the table.
    #[inline]
    pub fn header_style(mut self, style: Option<Style>) -> Self {
        self.header_style = style;
        self
    }

    /// Base style for the table.
    #[inline]
    pub fn footer_style(mut self, style: Option<Style>) -> Self {
        self.footer_style = style;
        self
    }

    /// Style for a selected row. The chosen selection must support
    /// row-selection for this to take effect.
    #[inline]
    pub fn select_row_style(mut self, select_style: Option<Style>) -> Self {
        self.select_row_style = select_style;
        self
    }

    /// Add the focus-style to the row-style if the table is focused.
    #[inline]
    pub fn show_row_focus(mut self, show: bool) -> Self {
        self.show_row_focus = show;
        self
    }

    /// Style for a selected column. The chosen selection must support
    /// column-selection for this to take effect.
    #[inline]
    pub fn select_column_style(mut self, select_style: Option<Style>) -> Self {
        self.select_column_style = select_style;
        self
    }

    /// Add the focus-style to the column-style if the table is focused.
    #[inline]
    pub fn show_column_focus(mut self, show: bool) -> Self {
        self.show_column_focus = show;
        self
    }

    /// Style for a selected cell. The chosen selection must support
    /// cell-selection for this to take effect.
    #[inline]
    pub fn select_cell_style(mut self, select_style: Option<Style>) -> Self {
        self.select_cell_style = select_style;
        self
    }

    /// Add the focus-style to the cell-style if the table is focused.
    #[inline]
    pub fn show_cell_focus(mut self, show: bool) -> Self {
        self.show_cell_focus = show;
        self
    }

    /// Style for a selected header cell. The chosen selection must
    /// support column-selection for this to take effect.
    #[inline]
    pub fn select_header_style(mut self, select_style: Option<Style>) -> Self {
        self.select_header_style = select_style;
        self
    }

    /// Add the focus-style to the header-style if the table is focused.
    #[inline]
    pub fn show_header_focus(mut self, show: bool) -> Self {
        self.show_header_focus = show;
        self
    }

    /// Style for a selected footer cell. The chosen selection must
    /// support column-selection for this to take effect.
    #[inline]
    pub fn select_footer_style(mut self, select_style: Option<Style>) -> Self {
        self.select_footer_style = select_style;
        self
    }

    /// Add the footer-style to the table-style if the table is focused.
    #[inline]
    pub fn show_footer_focus(mut self, show: bool) -> Self {
        self.show_footer_focus = show;
        self
    }

    /// This style will be patched onto the selection to indicate that
    /// the widget has the input focus.
    ///
    /// The selection must support some kind of selection for this to
    /// be effective.
    #[inline]
    pub fn focus_style(mut self, focus_style: Option<Style>) -> Self {
        self.focus_style = focus_style;
        self
    }

    /// Just some utility to help with debugging. Usually does nothing.
    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }
}

impl<'a, Selection> Table<'a, Selection> {
    // area_width or layout_width
    #[inline]
    fn total_width(&self, area_width: u16) -> u16 {
        if let Some(layout_width) = self.layout_width {
            layout_width
        } else if self.auto_layout_width {
            let mut width = 0;
            for w in &self.widths {
                match w {
                    Constraint::Min(v) => width += *v + self.column_spacing,
                    Constraint::Max(v) => width += *v + self.column_spacing,
                    Constraint::Length(v) => width += *v + self.column_spacing,
                    _ => unimplemented!("Invalid layout constraint."),
                }
            }
            width
        } else {
            area_width
        }
    }

    // Do the column-layout. Fill in missing columns, if necessary.
    #[inline]
    fn layout_columns(&self, width: u16) -> (u16, Rc<[Rect]>, Rc<[Rect]>) {
        let width = self.total_width(width);
        let area = Rect::new(0, 0, width, 0);

        let (layout, spacers) = Layout::horizontal(&self.widths)
            .flex(self.flex)
            .spacing(self.column_spacing)
            .split_with_spacers(area);

        (width, layout, spacers)
    }

    // Layout header/table/footer
    #[inline]
    fn layout_areas(&self, area: Rect) -> Rc<[Rect]> {
        let heights = vec![
            Constraint::Length(self.header.as_ref().map(|v| v.height).unwrap_or(0)),
            Constraint::Fill(1),
            Constraint::Length(self.footer.as_ref().map(|v| v.height).unwrap_or(0)),
        ];

        Layout::vertical(heights).split(area)
    }
}

impl<'a, Selection> StatefulWidgetRef for Table<'a, Selection>
where
    Selection: TableSelection,
{
    type State = TableState<Selection>;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let iter = self.data.iter();
        self.render_iter(iter, area, buf, state);
    }
}

impl<'a, Selection> StatefulWidget for Table<'a, Selection>
where
    Selection: TableSelection,
{
    type State = TableState<Selection>;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let iter = mem::take(&mut self.data).into_iter();
        self.render_iter(iter, area, buf, state);
    }
}

impl<'a, Selection> Table<'a, Selection>
where
    Selection: TableSelection,
{
    /// Render an Iterator over TableRowData.
    ///
    /// rows: If the row number is known, this can help.
    ///
    fn render_iter<'b>(
        &self,
        mut data: DataReprIter<'a, 'b>,
        area: Rect,
        buf: &mut Buffer,
        state: &mut TableState<Selection>,
    ) {
        if let Some(rows) = data.rows() {
            state.rows = rows;
        }
        state.columns = self.widths.len();
        state.area = area;

        // vertical layout
        let (hscroll_area, vscroll_area, inner_area) = layout_scroll(
            area,
            self.block.as_ref(),
            self.hscroll.as_ref(),
            self.vscroll.as_ref(),
        );
        state.inner = inner_area;

        let l_rows = self.layout_areas(inner_area);
        state.header_area = l_rows[0];
        state.table_area = l_rows[1];
        state.footer_area = l_rows[2];

        // horizontal layout
        let (width, l_columns, l_spacers) = self.layout_columns(state.table_area.width);
        self.calculate_column_areas(state.columns, l_columns.as_ref(), l_spacers.as_ref(), state);

        // set everything, so I don't have to care about unpainted areas later.
        buf.set_style(state.area, self.style);

        // render header & footer
        self.render_header(
            state.columns,
            width,
            l_columns.as_ref(),
            l_spacers.as_ref(),
            state.header_area,
            buf,
            state,
        );
        self.render_footer(
            state.columns,
            width,
            l_columns.as_ref(),
            l_spacers.as_ref(),
            state.footer_area,
            buf,
            state,
        );

        // render table
        state.row_areas.clear();
        state.vscroll.set_page_len(0);
        state.hscroll.set_page_len(area.width as usize);

        let mut row_buf = Buffer::empty(Rect::new(0, 0, width, 1));
        let mut row = None;
        let mut row_y = state.table_area.y;
        let mut row_heights = Vec::new();
        #[cfg(debug_assertions)]
        let mut insane_offset = false;

        let mut ctx = RTableContext {
            focus: state.focus.get(),
            selected_cell: false,
            selected_row: false,
            selected_column: false,
            style: self.style,
            row_style: None,
            select_style: None,
            space_area: Default::default(),
            non_exhaustive: NonExhaustive,
        };

        if data.nth(state.vscroll.offset()) {
            row = Some(state.vscroll.offset());
            loop {
                ctx.row_style = data.row_style();
                // We render each row to a temporary buffer.
                // For ease of use we start each row at 0,0.
                // We still only render at least partially visible cells.
                let render_row_area = Rect::new(0, 0, width, data.row_height());
                row_buf.resize(render_row_area);
                if let Some(row_style) = ctx.row_style {
                    row_buf.set_style(render_row_area, row_style);
                } else {
                    row_buf.set_style(render_row_area, self.style);
                }
                row_heights.push(render_row_area.height);

                // Target area for the finished row.
                let visible_row_area = Rect::new(
                    state.table_area.x,
                    row_y,
                    state.table_area.width,
                    max(data.row_height(), 1),
                )
                .intersection(state.table_area);
                state.row_areas.push(visible_row_area);
                state.vscroll.set_page_len(state.vscroll.page_len() + 1);

                let mut col = 0;
                loop {
                    if col >= state.columns {
                        break;
                    }

                    let render_cell_area = Rect::new(
                        l_columns[col].x,
                        0,
                        l_columns[col].width,
                        render_row_area.height,
                    );
                    ctx.space_area = Rect::new(
                        l_spacers[col + 1].x,
                        0,
                        l_spacers[col + 1].width,
                        render_row_area.height,
                    );

                    ctx.select_style = if state.selection.is_selected_cell(col, row.expect("row")) {
                        ctx.selected_cell = true;
                        ctx.selected_row = false;
                        ctx.selected_column = false;
                        self.patch_select(
                            self.select_cell_style,
                            state.focus.get(),
                            self.show_cell_focus,
                        )
                    } else if state.selection.is_selected_row(row.expect("row")) {
                        ctx.selected_cell = false;
                        ctx.selected_row = true;
                        ctx.selected_column = false;
                        // use a fallback if no row-selected style is set.
                        if self.select_row_style.is_some() {
                            self.patch_select(
                                self.select_row_style,
                                state.focus.get(),
                                self.show_row_focus,
                            )
                        } else {
                            self.patch_select(
                                Some(revert_style(self.style)),
                                state.focus.get(),
                                self.show_row_focus,
                            )
                        }
                    } else if state.selection.is_selected_column(col) {
                        ctx.selected_cell = false;
                        ctx.selected_row = false;
                        ctx.selected_column = true;
                        self.patch_select(
                            self.select_column_style,
                            state.focus.get(),
                            self.show_column_focus,
                        )
                    } else {
                        ctx.selected_cell = false;
                        ctx.selected_row = false;
                        ctx.selected_column = false;
                        None
                    };

                    // partially visible?
                    if render_cell_area.right() > state.hscroll.offset as u16
                        || render_cell_area.left() < state.hscroll.offset as u16 + area.width
                    {
                        if let Some(select_style) = ctx.select_style {
                            row_buf.set_style(render_cell_area, select_style);
                            row_buf.set_style(ctx.space_area, select_style);
                        }
                        data.render_cell(&ctx, col, render_cell_area, &mut row_buf);
                    }

                    col += 1;
                }

                // render shifted and clipped row.
                transfer_buffer(
                    &mut row_buf,
                    state.hscroll.offset() as u16,
                    visible_row_area,
                    buf,
                );

                if visible_row_area.bottom() >= state.table_area.bottom() {
                    break;
                }
                if !data.nth(0) {
                    break;
                }
                row = Some(row.expect("row") + 1);
                row_y += render_row_area.height;
            }
        } else {
            // can only guess whether the skip failed completely or partially.
            // so don't alter row here.

            // if this first skip fails all bets are off.
            if data.rows().is_none() || data.rows() == Some(0) {
                // this is ok
            } else {
                #[cfg(debug_assertions)]
                {
                    insane_offset = true;
                }
            }
        }

        // maximum offsets
        #[allow(unused_variables)]
        let algorithm;
        #[allow(unused_assignments)]
        {
            if let Some(rows) = data.rows() {
                algorithm = 0;
                // skip to a guess for the last page.
                // the guess uses row-height is 1, which may read a few more lines than
                // absolutely necessary.
                let skip_rows = rows
                    .saturating_sub(row.map_or(0, |v| v + 1))
                    .saturating_sub(state.table_area.height as usize);
                // if we can still skip some rows, then the data so far is useless.
                if skip_rows > 0 {
                    row_heights.clear();
                }
                let nth_row = skip_rows;
                // collect the remaining row-heights.
                if data.nth(nth_row) {
                    row = Some(row.map_or(nth_row, |row| row + nth_row + 1));
                    loop {
                        row_heights.push(data.row_height());
                        // don't need more.
                        if row_heights.len() > state.table_area.height as usize {
                            row_heights.remove(0);
                        }
                        if !data.nth(0) {
                            break;
                        }
                        row = Some(row.expect("row") + 1);
                        // if the given number of rows is too small, we would overshoot here.
                        if row.expect("row") > rows {
                            break;
                        }
                    }
                    // we break before to have an accurate last page.
                    // but we still want to report an error, if the count is off.
                    while data.nth(0) {
                        row = Some(row.expect("row") + 1);
                    }
                } else {
                    // skip failed, maybe again?
                    // leave everything as is and report later.
                }

                state.rows = rows;
                state._counted_rows = row.map_or(0, |v| v + 1);

                // have we got a page worth of data?
                if let Some(last_page) = state.calc_last_page(row_heights) {
                    state.vscroll.set_max_offset(state.rows - last_page);
                } else {
                    // we don't have enough data to establish the last page.
                    // either there are not enough rows or the given row-count
                    // was off. make a guess.
                    state.vscroll.set_max_offset(
                        state.rows.saturating_sub(state.table_area.height as usize),
                    );
                }
            } else if self.no_row_count {
                algorithm = 1;

                // We need to feel out a bit beyond the page, otherwise
                // we can't really stabilize the row count and the
                // display starts flickering.
                if row.is_some() {
                    if data.nth(0) {
                        // try one past page
                        row = Some(row.expect("row") + 1);
                        if data.nth(0) {
                            // have an unknown number of rows left.
                            row = Some(usize::MAX - 1);
                        }
                    }
                }

                state.rows = row.map_or(0, |v| v + 1);
                state._counted_rows = row.map_or(0, |v| v + 1);
                // rough estimate
                state.vscroll.set_max_offset(usize::MAX - 1);
                if state.vscroll.page_len() == 0 {
                    state.vscroll.set_page_len(state.table_area.height as usize);
                }
            } else {
                algorithm = 2;

                // Read all the rest to establish the exact row-count.
                while data.nth(0) {
                    row_heights.push(data.row_height());
                    // don't need more info. drop the oldest.
                    if row_heights.len() > state.table_area.height as usize {
                        row_heights.remove(0);
                    }
                    row = Some(row.map_or(0, |v| v + 1));
                }

                state.rows = row.map_or(0, |v| v + 1);
                state._counted_rows = row.map_or(0, |v| v + 1);

                // have we got a page worth of data?
                if let Some(last_page) = state.calc_last_page(row_heights) {
                    state.vscroll.set_max_offset(state.rows - last_page);
                } else {
                    state.vscroll.set_max_offset(0);
                }
            }
        }
        {
            state
                .hscroll
                .set_max_offset(width.saturating_sub(state.table_area.width) as usize);
        }

        // render block+scroll
        self.block.render_ref(area, buf);
        if let Some(hscroll) = self.hscroll.as_ref() {
            hscroll.render_ref(hscroll_area, buf, &mut state.hscroll);
        }
        if let Some(vscroll) = self.vscroll.as_ref() {
            vscroll.render_ref(vscroll_area, buf, &mut state.vscroll);
        }

        #[cfg(debug_assertions)]
        {
            use std::fmt::Write;
            let mut msg = String::new();
            if insane_offset {
                _= write!(msg,
                          "Table::render:\n        offset {}\n        rows {}\n        iter-rows {}max\n    don't match up\nCode X{}X\n",
                          state.vscroll.offset(), state.rows, state._counted_rows, algorithm
                );
            }
            if state.rows != state._counted_rows {
                _ = write!(
                    msg,
                    "Table::render:\n    rows {} don't match\n    iterated rows {}\nCode X{}X\n",
                    state.rows, state._counted_rows, algorithm
                );
            }
            if !msg.is_empty() {
                warn!("{}", &msg);
                Text::from(msg)
                    .white()
                    .on_red()
                    .render(state.table_area, buf);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_footer(
        &self,
        columns: usize,
        width: u16,
        l_columns: &[Rect],
        l_spacers: &[Rect],
        area: Rect,
        buf: &mut Buffer,
        state: &mut TableState<Selection>,
    ) {
        if let Some(footer) = &self.footer {
            let render_row_area = Rect::new(0, 0, width, footer.height);
            let mut row_buf = Buffer::empty(render_row_area);

            row_buf.set_style(render_row_area, self.style);
            if let Some(footer_style) = footer.style {
                row_buf.set_style(render_row_area, footer_style);
            } else if let Some(footer_style) = self.footer_style {
                row_buf.set_style(render_row_area, footer_style);
            }

            let mut col = 0;
            loop {
                if col >= columns {
                    break;
                }

                let render_cell_area =
                    Rect::new(l_columns[col].x, 0, l_columns[col].width, area.height);
                let render_space_area = Rect::new(
                    l_spacers[col + 1].x,
                    0,
                    l_spacers[col + 1].width,
                    area.height,
                );

                if state.selection.is_selected_column(col) {
                    if let Some(selected_style) = self.patch_select(
                        self.select_footer_style,
                        state.focus.get(),
                        self.show_footer_focus,
                    ) {
                        row_buf.set_style(render_cell_area, selected_style);
                        row_buf.set_style(render_space_area, selected_style);
                    }
                };

                // partially visible?
                if render_cell_area.right() > state.hscroll.offset as u16
                    || render_cell_area.left() < state.hscroll.offset as u16 + area.width
                {
                    if let Some(cell) = footer.cells.get(col) {
                        if let Some(cell_style) = cell.style {
                            row_buf.set_style(render_cell_area, cell_style);
                        }
                        cell.content.clone().render(render_cell_area, &mut row_buf);
                    }
                }

                col += 1;
            }

            // render shifted and clipped row.
            transfer_buffer(&mut row_buf, state.hscroll.offset() as u16, area, buf);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_header(
        &self,
        columns: usize,
        width: u16,
        l_columns: &[Rect],
        l_spacers: &[Rect],
        area: Rect,
        buf: &mut Buffer,
        state: &mut TableState<Selection>,
    ) {
        if let Some(header) = &self.header {
            let render_row_area = Rect::new(0, 0, width, header.height);
            let mut row_buf = Buffer::empty(render_row_area);

            row_buf.set_style(render_row_area, self.style);
            if let Some(header_style) = header.style {
                row_buf.set_style(render_row_area, header_style);
            } else if let Some(header_style) = self.header_style {
                row_buf.set_style(render_row_area, header_style);
            }

            let mut col = 0;
            loop {
                if col >= columns {
                    break;
                }

                let render_cell_area =
                    Rect::new(l_columns[col].x, 0, l_columns[col].width, area.height);
                let render_space_area = Rect::new(
                    l_spacers[col + 1].x,
                    0,
                    l_spacers[col + 1].width,
                    area.height,
                );

                if state.selection.is_selected_column(col) {
                    if let Some(selected_style) = self.patch_select(
                        self.select_header_style,
                        state.focus.get(),
                        self.show_header_focus,
                    ) {
                        row_buf.set_style(render_cell_area, selected_style);
                        row_buf.set_style(render_space_area, selected_style);
                    }
                };

                // partially visible?
                if render_cell_area.right() > state.hscroll.offset as u16
                    || render_cell_area.left() < state.hscroll.offset as u16 + area.width
                {
                    if let Some(cell) = header.cells.get(col) {
                        if let Some(cell_style) = cell.style {
                            row_buf.set_style(render_cell_area, cell_style);
                        }
                        cell.content.clone().render(render_cell_area, &mut row_buf);
                    }
                }

                col += 1;
            }

            // render shifted and clipped row.
            transfer_buffer(&mut row_buf, state.hscroll.offset() as u16, area, buf);
        }
    }

    fn calculate_column_areas(
        &self,
        columns: usize,
        l_columns: &[Rect],
        l_spacers: &[Rect],
        state: &mut TableState<Selection>,
    ) {
        state.column_areas.clear();
        state.column_layout.clear();

        let mut col = 0;
        let shift = state.hscroll.offset() as isize;
        loop {
            if col >= columns {
                break;
            }

            state.column_layout.push(Rect::new(
                l_columns[col].x,
                0,
                l_columns[col].width + l_spacers[col + 1].width,
                0,
            ));

            let cell_x1 = l_columns[col].x as isize;
            let cell_x2 =
                (l_columns[col].x + l_columns[col].width + l_spacers[col + 1].width) as isize;

            let squish_x1 = cell_x1.saturating_sub(shift);
            let squish_x2 = cell_x2.saturating_sub(shift);

            let abs_x1 = max(0, squish_x1) as u16;
            let abs_x2 = max(0, squish_x2) as u16;

            let v_area = Rect::new(
                state.table_area.x + abs_x1,
                state.table_area.y,
                abs_x2 - abs_x1,
                state.table_area.height,
            );
            state
                .column_areas
                .push(v_area.intersection(state.table_area));

            col += 1;
        }
    }

    fn patch_select(&self, style: Option<Style>, focus: bool, show: bool) -> Option<Style> {
        if let Some(style) = style {
            if let Some(focus_style) = self.focus_style {
                if focus && show {
                    Some(style.patch(focus_style))
                } else {
                    Some(style)
                }
            } else {
                Some(style)
            }
        } else {
            None
        }
    }
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            style: Default::default(),
            header_style: None,
            footer_style: None,
            select_row_style: None,
            select_column_style: None,
            select_cell_style: None,
            select_header_style: None,
            select_footer_style: None,
            show_row_focus: false,
            show_column_focus: false,
            show_cell_focus: false,
            show_header_focus: false,
            show_footer_focus: false,
            focus_style: None,
            non_exhaustive: NonExhaustive,
        }
    }
}

impl<Selection: Default> Default for TableState<Selection> {
    fn default() -> Self {
        Self {
            focus: Default::default(),
            area: Default::default(),
            inner: Default::default(),
            header_area: Default::default(),
            table_area: Default::default(),
            row_areas: Default::default(),
            column_areas: Default::default(),
            column_layout: Default::default(),
            footer_area: Default::default(),
            rows: Default::default(),
            _counted_rows: Default::default(),
            columns: Default::default(),
            vscroll: Default::default(),
            hscroll: Default::default(),
            selection: Default::default(),
            mouse: Default::default(),
            non_exhaustive: NonExhaustive,
        }
    }
}

impl<Selection> HasFocusFlag for TableState<Selection> {
    #[inline]
    fn focus(&self) -> &FocusFlag {
        &self.focus
    }

    #[inline]
    fn area(&self) -> Rect {
        self.area
    }
}

impl<Selection> TableState<Selection> {
    fn calc_last_page(&self, mut row_heights: Vec<u16>) -> Option<usize> {
        let mut sum_heights = 0;
        let mut n_rows = 0;
        while let Some(h) = row_heights.pop() {
            sum_heights += h;
            n_rows += 1;
            if sum_heights >= self.table_area.height {
                break;
            }
        }

        if sum_heights < self.table_area.height {
            None
        } else {
            Some(n_rows)
        }
    }
}

// Baseline
impl<Selection> TableState<Selection> {
    /// Number of rows.
    #[inline]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    #[inline]
    pub fn columns(&self) -> usize {
        self.columns
    }
}

// Table areas
impl<Selection> TableState<Selection> {
    /// Returns the whole row-area and the cell-areas for the
    /// given row, if it is visible.
    ///
    /// Attention: These areas might be 0-length if the column is scrolled
    /// beyond the table-area.
    ///
    /// See: [TableState::scroll_to]
    pub fn row_cells(&self, row: usize) -> Option<(Rect, Vec<Rect>)> {
        if row < self.vscroll.offset() || row >= self.vscroll.offset() + self.vscroll.page_len() {
            return None;
        }

        let mut areas = Vec::new();

        let r = self.row_areas[row];
        for c in &self.column_areas {
            areas.push(Rect::new(c.x, r.y, c.width, r.height));
        }

        Some((r, areas))
    }

    /// Cell at given position.
    pub fn cell_at_clicked(&self, pos: (u16, u16)) -> Option<(usize, usize)> {
        let col = self.column_at_clicked(pos);
        let row = self.row_at_clicked(pos);

        match (col, row) {
            (Some(col), Some(row)) => Some((col, row)),
            _ => None,
        }
    }

    /// Column at given position.
    pub fn column_at_clicked(&self, pos: (u16, u16)) -> Option<usize> {
        rat_event::util::column_at_clicked(&self.column_areas, pos.0)
    }

    /// Row at given position.
    pub fn row_at_clicked(&self, pos: (u16, u16)) -> Option<usize> {
        rat_event::util::row_at_clicked(&self.row_areas, pos.1).map(|v| self.vscroll.offset() + v)
    }

    /// Cell when dragging. Position can be outside the table area.
    /// See [row_at_drag](TableState::row_at_drag), [col_at_drag](TableState::column_at_drag)
    pub fn cell_at_drag(&self, pos: (u16, u16)) -> (usize, usize) {
        let col = self.column_at_drag(pos);
        let row = self.row_at_drag(pos);

        (col, row)
    }

    /// Row when dragging. Position can be outside the table area.
    /// If the position is above the table-area this returns offset - #rows.
    /// If the position is below the table-area this returns offset + page_len + #rows.
    ///
    /// This doesn't account for the row-height of the actual rows outside
    /// the table area, just assumes '1'.
    pub fn row_at_drag(&self, pos: (u16, u16)) -> usize {
        match rat_event::util::row_at_drag(self.table_area, &self.row_areas, pos.1) {
            Ok(v) => self.vscroll.offset() + v,
            Err(v) if v <= 0 => self.vscroll.offset().saturating_sub((-v) as usize),
            Err(v) => self.vscroll.offset() + self.row_areas.len() + v as usize,
        }
    }

    /// Column when dragging. Position can be outside the table area.
    /// If the position is left of the table area this returns offset - 1.
    /// If the position is right of the table area this returns offset + page_width + 1.
    pub fn column_at_drag(&self, pos: (u16, u16)) -> usize {
        match rat_event::util::column_at_drag(self.table_area, &self.column_areas, pos.0) {
            Ok(v) => v,
            Err(_) => todo!(),
        }
    }
}

// Offset related.
impl<Selection: TableSelection> TableState<Selection> {
    /// Sets both offsets to 0.
    pub fn clear_offset(&mut self) {
        self.vscroll.set_offset(0);
        self.hscroll.set_offset(0);
    }

    /// Maximum offset that is accessible with scrolling.
    ///
    /// This is shorter than the length by whatever fills the last page.
    /// This is the base for the scrollbar content_length.
    pub fn row_max_offset(&self) -> usize {
        self.vscroll.max_offset()
    }

    /// Current vertical offset.
    pub fn row_offset(&self) -> usize {
        self.vscroll.offset()
    }

    /// Change the vertical offset.
    ///
    /// Due to overscroll it's possible that this is an invalid offset for the widget.
    /// The widget must deal with this situation.
    ///
    /// The widget returns true if the offset changed at all.
    pub fn set_row_offset(&mut self, offset: usize) -> bool {
        self.vscroll.set_offset(offset)
    }

    /// Vertical page-size at the current offset.
    pub fn page_len(&self) -> usize {
        self.vscroll.page_len()
    }

    /// Suggested scroll per scroll-event.
    pub fn row_scroll_by(&self) -> usize {
        self.vscroll.scroll_by()
    }

    /// Maximum offset that is accessible with scrolling.
    ///
    /// This is shorter than the length of the content by whatever fills the last page.
    /// This is the base for the scrollbar content_length.
    pub fn x_max_offset(&self) -> usize {
        self.hscroll.max_offset()
    }

    /// Current horizontal offset.
    pub fn x_offset(&self) -> usize {
        self.hscroll.offset()
    }

    /// Change the horizontal offset.
    ///
    /// Due to overscroll it's possible that this is an invalid offset for the widget.
    /// The widget must deal with this situation.
    ///
    /// The widget returns true if the offset changed at all.
    pub fn set_x_offset(&mut self, offset: usize) -> bool {
        self.hscroll.set_offset(offset)
    }

    /// Horizontal page-size at the current offset.
    pub fn page_width(&self) -> usize {
        self.hscroll.page_len()
    }

    /// Suggested scroll per scroll-event.
    pub fn x_scroll_by(&self) -> usize {
        self.hscroll.scroll_by()
    }

    /// Ensures that the selected item is visible.
    pub fn scroll_to_selected(&mut self) -> bool {
        if let Some(selected) = self.selection.lead_selection() {
            let c = self.scroll_to_x(selected.0);
            let r = self.scroll_to_row(selected.1);
            r || c
        } else {
            false
        }
    }

    /// Ensures that the given row is visible.
    pub fn scroll_to_row(&mut self, pos: usize) -> bool {
        if pos >= self.row_offset() + self.page_len() {
            self.set_row_offset(pos - self.page_len() + 1)
        } else if pos < self.row_offset() {
            self.set_row_offset(pos)
        } else {
            false
        }
    }

    /// Ensures that the given column is completely visible.
    pub fn scroll_to_col(&mut self, pos: usize) -> bool {
        if let Some(col) = self.column_layout.get(pos) {
            if (col.left() as usize) < self.x_offset() {
                self.set_x_offset(col.x as usize)
            } else if (col.right() as usize) >= self.x_offset() + self.page_width() {
                self.set_x_offset(col.right() as usize - self.page_width())
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Ensures that the given cell is visible.
    pub fn scroll_to_x(&mut self, pos: usize) -> bool {
        if pos >= self.x_offset() + self.page_width() {
            self.set_x_offset(pos - self.page_width() + 1)
        } else if pos < self.x_offset() {
            self.set_x_offset(pos)
        } else {
            false
        }
    }

    /// Reduce the row-offset by n.
    pub fn scroll_up(&mut self, n: usize) -> bool {
        self.vscroll.scroll_up(n)
    }

    /// Increase the row-offset by n.
    pub fn scroll_down(&mut self, n: usize) -> bool {
        self.vscroll.scroll_down(n)
    }

    /// Reduce the col-offset by n.
    pub fn scroll_left(&mut self, n: usize) -> bool {
        self.hscroll.scroll_left(n)
    }

    /// Increase the col-offset by n.
    pub fn scroll_right(&mut self, n: usize) -> bool {
        self.hscroll.scroll_right(n)
    }
}

impl TableState<RowSelection> {
    /// Update the state to match adding items.
    /// This corrects the number of rows, offset and selection.
    pub fn items_added(&mut self, pos: usize, n: usize) {
        self.rows += n;
        self.vscroll.items_added(pos, n);
        self.selection.items_added(pos, n);
    }

    /// Update the state to match removing items.
    /// This corrects the number of rows, offset and selection.
    pub fn items_removed(&mut self, pos: usize, n: usize) {
        self.rows -= n;
        self.vscroll.items_removed(pos, n);
        self.selection.items_removed(pos, n);
    }

    /// When scrolling the table, change the selection instead of the offset.
    #[inline]
    pub fn set_scroll_selection(&mut self, scroll: bool) {
        self.selection.set_scroll_selected(scroll);
    }

    /// Clear the selection.
    #[inline]
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Anything selected?
    #[inline]
    pub fn has_selection(&mut self) -> bool {
        self.selection.has_selection()
    }

    /// Selected row.
    #[inline]
    pub fn selected(&self) -> Option<usize> {
        self.selection.selected()
    }

    /// Select the row.
    #[inline]
    pub fn select(&mut self, row: Option<usize>) -> bool {
        self.selection.select(row)
    }

    /// Scroll delivers a value between 0 and max_offset as offset.
    /// This remaps the ratio to the selection with a range 0..row_len.
    ///
    pub(crate) fn remap_offset_selection(&self, offset: usize) -> usize {
        if self.vscroll.max_offset() > 0 {
            (self.rows * offset) / self.vscroll.max_offset()
        } else {
            0 // ???
        }
    }

    /// Move the selection to the given row.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_to(&mut self, row: usize) -> bool {
        let r = self.selection.move_to(row, self.rows.saturating_sub(1));
        let s = self.scroll_to_row(self.selection.selected().expect("row"));
        r || s
    }

    /// Move the selection up n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_up(&mut self, n: usize) -> bool {
        let r = self.selection.move_up(n, self.rows.saturating_sub(1));
        let s = self.scroll_to_row(self.selection.selected().expect("row"));
        r || s
    }

    /// Move the selection down n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_down(&mut self, n: usize) -> bool {
        let r = self.selection.move_down(n, self.rows.saturating_sub(1));
        let s = self.scroll_to_row(self.selection.selected().expect("row"));
        r || s
    }
}

impl TableState<RowSetSelection> {
    /// Clear the selection.
    #[inline]
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Anything selected?
    #[inline]
    pub fn has_selection(&mut self) -> bool {
        self.selection.has_selection()
    }

    /// Selected rows.
    #[inline]
    pub fn selected(&self) -> HashSet<usize> {
        self.selection.selected()
    }

    /// Change the lead-selection. Limits the value to the number of rows.
    /// If extend is false the current selection is cleared and both lead and
    /// anchor are set to the given value.
    /// If extend is true, the anchor is kept where it is and lead is changed.
    /// Everything in the range `anchor..lead` is selected. It doesn't matter
    /// if anchor < lead.
    #[inline]
    pub fn set_lead(&mut self, row: Option<usize>, extend: bool) -> bool {
        self.selection.set_lead(row, extend)
    }

    /// Current lead.
    #[inline]
    pub fn lead(&self) -> Option<usize> {
        self.selection.lead()
    }

    /// Current anchor.
    #[inline]
    pub fn anchor(&self) -> Option<usize> {
        self.selection.anchor()
    }

    /// Retire the current anchor/lead selection to the set of selected rows.
    /// Resets lead and anchor and starts a new selection round.
    #[inline]
    pub fn retire_selection(&mut self) {
        self.selection.retire_selection();
    }

    /// Add to selection. Only works for retired selections, not for the
    /// active anchor-lead range.
    #[inline]
    pub fn add_selected(&mut self, idx: usize) {
        self.selection.add(idx);
    }

    /// Remove from selection. Only works for retired selections, not for the
    /// active anchor-lead range.
    #[inline]
    pub fn remove_selected(&mut self, idx: usize) {
        self.selection.remove(idx);
    }

    /// Move the selection to the given row.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_to(&mut self, row: usize, extend: bool) -> bool {
        let r = self
            .selection
            .move_to(row, self.rows.saturating_sub(1), extend);
        let s = self.scroll_to_row(self.selection.lead().expect("row"));
        r || s
    }

    /// Move the selection up n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_up(&mut self, n: usize, extend: bool) -> bool {
        let r = self
            .selection
            .move_up(n, self.rows.saturating_sub(1), extend);
        let s = self.scroll_to_row(self.selection.lead().expect("row"));
        r || s
    }

    /// Move the selection down n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_down(&mut self, n: usize, extend: bool) -> bool {
        let r = self
            .selection
            .move_down(n, self.rows.saturating_sub(1), extend);
        let s = self.scroll_to_row(self.selection.lead().expect("row"));
        r || s
    }
}

impl TableState<CellSelection> {
    #[inline]
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    #[inline]
    pub fn has_selection(&mut self) -> bool {
        self.selection.has_selection()
    }

    /// Selected cell.
    #[inline]
    pub fn selected(&self) -> Option<(usize, usize)> {
        self.selection.selected()
    }

    /// Select a cell.
    #[inline]
    pub fn select_cell(&mut self, select: Option<(usize, usize)>) -> bool {
        self.selection.select_cell(select)
    }

    /// Select a row. Column stays the same.
    #[inline]
    pub fn select_row(&mut self, row: Option<usize>) -> bool {
        if let Some(row) = row {
            self.selection
                .select_row(Some(min(row, self.rows.saturating_sub(1))))
        } else {
            self.selection.select_row(None)
        }
    }

    /// Select a column, row stays the same.
    #[inline]
    pub fn select_column(&mut self, column: Option<usize>) -> bool {
        if let Some(column) = column {
            self.selection
                .select_column(Some(min(column, self.columns.saturating_sub(1))))
        } else {
            self.selection.select_column(None)
        }
    }

    /// Select a cell, limit to maximum.
    #[inline]
    pub fn move_to(&mut self, select: (usize, usize)) -> bool {
        let r = self.selection.move_to(
            select,
            (self.columns.saturating_sub(1), self.rows.saturating_sub(1)),
        );
        let s = self.scroll_to_selected();
        r || s
    }

    /// Select a row, limit to maximum.
    #[inline]
    pub fn move_to_row(&mut self, row: usize) -> bool {
        let r = self.selection.move_to_row(row, self.rows.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }

    /// Select a cell, clamp between 0 and maximum.
    #[inline]
    pub fn move_to_col(&mut self, col: usize) -> bool {
        let r = self
            .selection
            .move_to_col(col, self.columns.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }

    /// Move the selection up n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_up(&mut self, n: usize) -> bool {
        let r = self.selection.move_up(n, self.rows.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }

    /// Move the selection down n rows.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_down(&mut self, n: usize) -> bool {
        let r = self.selection.move_down(n, self.rows.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }

    /// Move the selection left n columns.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_left(&mut self, n: usize) -> bool {
        let r = self.selection.move_left(n, self.columns.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }

    /// Move the selection right n columns.
    /// Ensures the row is visible afterwards.
    #[inline]
    pub fn move_right(&mut self, n: usize) -> bool {
        let r = self.selection.move_right(n, self.columns.saturating_sub(1));
        let s = self.scroll_to_selected();
        r || s
    }
}

impl<Selection> HandleEvent<crossterm::event::Event, DoubleClick, DoubleClickOutcome>
    for TableState<Selection>
{
    /// Handles double-click events on the table.
    fn handle(
        &mut self,
        event: &crossterm::event::Event,
        _keymap: DoubleClick,
    ) -> DoubleClickOutcome {
        match event {
            ct_event!(mouse any for m) if self.mouse.doubleclick(self.table_area, m) => {
                if let Some((col, row)) = self.cell_at_clicked((m.column, m.row)) {
                    DoubleClickOutcome::ClickClick(col, row)
                } else {
                    DoubleClickOutcome::NotUsed
                }
            }
            _ => DoubleClickOutcome::NotUsed,
        }
    }
}

/// Handle all events for recognizing double-clicks.
pub fn handle_doubleclick_events<Selection: TableSelection>(
    state: &mut TableState<Selection>,
    event: &crossterm::event::Event,
) -> DoubleClickOutcome {
    state.handle(event, DoubleClick)
}

impl<Selection: TableSelection> HandleEvent<crossterm::event::Event, EditKeys, EditOutcome>
    for TableState<Selection>
where
    Self: HandleEvent<crossterm::event::Event, Regular, Outcome>,
{
    fn handle(&mut self, event: &crossterm::event::Event, _keymap: EditKeys) -> EditOutcome {
        match event {
            ct_event!(keycode press Insert) => EditOutcome::Insert,
            ct_event!(keycode press Delete) => EditOutcome::Remove,
            ct_event!(keycode press Enter) => EditOutcome::Edit,
            ct_event!(keycode press Down) => {
                if let Some((_column, row)) = self.selection.lead_selection() {
                    if row == self.rows().saturating_sub(1) {
                        return EditOutcome::Append;
                    }
                }
                <Self as HandleEvent<_, Regular, Outcome>>::handle(self, event, Regular).into()
            }

            ct_event!(keycode release  Insert)
            | ct_event!(keycode release Delete)
            | ct_event!(keycode release Enter)
            | ct_event!(keycode release Down) => EditOutcome::Unchanged,

            _ => <Self as HandleEvent<_, Regular, Outcome>>::handle(self, event, Regular).into(),
        }
    }
}

/// Handle all events.
/// Text events are only processed if focus is true.
/// Mouse events are processed if they are in range.
pub fn handle_edit_events<Selection: TableSelection>(
    state: &mut TableState<Selection>,
    focus: bool,
    event: &crossterm::event::Event,
) -> EditOutcome
where
    TableState<Selection>: HandleEvent<crossterm::event::Event, Regular, Outcome>,
    TableState<Selection>: HandleEvent<crossterm::event::Event, MouseOnly, Outcome>,
{
    state.focus.set(focus);
    state.handle(event, EditKeys)
}
