use crate::data::render_tablestate::render_tablestate;
use anyhow::anyhow;
use crossterm::cursor::{DisableBlinking, EnableBlinking, SetCursorStyle};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, KeyCode,
    KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use format_num_pattern::NumberFormat;
use log::debug;
use rat_event::{ct_event, FocusKeys, HandleEvent};
use rat_ftable::event::Outcome;
use rat_ftable::selection::NoSelection;
use rat_ftable::textdata::{Cell, Row};
use rat_ftable::{FTable, FTableState, TableData, TableDataIter};
use rat_input::layout_edit::{layout_edit, EditConstraint, LayoutEdit};
use rat_input::statusline::{StatusLine, StatusLineState};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Span;
use ratatui::{Frame, Terminal};
use std::fs;
use std::io::{stdout, Stdout};
use std::iter::Enumerate;
use std::slice::Iter;
use std::time::{Duration, SystemTime};

mod data;

fn main() -> Result<(), anyhow::Error> {
    setup_logging()?;

    let mut data = Data {
        table_data: data::DATA
            .iter()
            .map(|v| Sample {
                text: *v,
                num1: rand::random(),
                num2: rand::random(),
                check: rand::random(),
            })
            .take(100_010)
            .collect(),
    };

    let mut state = State {
        table: Default::default(),
        report_rows: None,
        edit: Default::default(),
        status: Default::default(),
    };
    state.status.status(0, "Ctrl+Q to quit.");

    run_ui(&mut data, &mut state)
}

fn setup_logging() -> Result<(), anyhow::Error> {
    fs::remove_file("log.log")?;
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}]\n",
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("log.log")?)
        .apply()?;
    Ok(())
}

struct Sample {
    pub(crate) text: &'static str,
    pub(crate) num1: f32,
    pub(crate) num2: f32,
    pub(crate) check: bool,
}

struct Data {
    pub(crate) table_data: Vec<Sample>,
}

struct State {
    pub(crate) table: FTableState<NoSelection>,
    pub(crate) report_rows: Option<usize>,
    pub(crate) edit: LayoutEdit,
    pub(crate) status: StatusLineState,
}

fn run_ui(data: &mut Data, state: &mut State) -> Result<(), anyhow::Error> {
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    stdout().execute(EnableBlinking)?;
    stdout().execute(SetCursorStyle::BlinkingBar)?;
    stdout().execute(EnableBracketedPaste)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    repaint_ui(&mut terminal, data, state)?;

    let r = 'l: loop {
        let o = match crossterm::event::poll(Duration::from_millis(10)) {
            Ok(true) => {
                let event = match crossterm::event::read() {
                    Ok(v) => v,
                    Err(e) => break 'l Err(anyhow!(e)),
                };
                match handle_event(event, data, state) {
                    Ok(v) => v,
                    Err(e) => break 'l Err(e),
                }
            }
            Ok(false) => continue,
            Err(e) => break 'l Err(anyhow!(e)),
        };

        match o {
            Outcome::Changed => {
                match repaint_ui(&mut terminal, data, state) {
                    Ok(_) => {}
                    Err(e) => break 'l Err(e),
                };
            }
            _ => {
                // noop
            }
        }
    };

    disable_raw_mode()?;
    stdout().execute(DisableBracketedPaste)?;
    stdout().execute(SetCursorStyle::DefaultUserShape)?;
    stdout().execute(DisableBlinking)?;
    stdout().execute(DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;

    r
}

fn repaint_ui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    data: &mut Data,
    state: &mut State,
) -> Result<(), anyhow::Error> {
    terminal.hide_cursor()?;

    _ = terminal.draw(|frame| {
        repaint_tui(frame, data, state);
    });

    Ok(())
}

fn repaint_tui(frame: &mut Frame<'_>, data: &mut Data, state: &mut State) {
    let t0 = SystemTime::now();
    let area = frame.size();

    let l1 = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(area);

    repaint_table(frame, l1[0], data, state);

    let status1 = StatusLine::new()
        .layout([
            Constraint::Fill(1),
            Constraint::Length(17),
            Constraint::Length(17),
        ])
        .styles([
            Style::default().black().on_dark_gray(),
            Style::default().white().on_blue(),
            Style::default().white().on_light_blue(),
        ]);

    let el = t0.elapsed().unwrap_or(Duration::from_nanos(0));
    state
        .status
        .status(1, format!("Render {:?}", el).to_string());
    frame.render_stateful_widget(status1, l1[1], &mut state.status);
}

fn handle_event(
    event: crossterm::event::Event,
    data: &mut Data,
    state: &mut State,
) -> Result<Outcome, anyhow::Error> {
    let t0 = SystemTime::now();

    let r = {
        use crossterm::event::Event;
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }) => {
                return Err(anyhow!("quit"));
            }
            Event::Resize(_, _) => return Ok(Outcome::Changed),
            _ => {}
        }

        let r = handle_table(&event, data, state)?;

        r
    };

    let el = t0.elapsed().unwrap_or(Duration::from_nanos(0));
    state
        .status
        .status(2, format!("Handle {:?}", el).to_string());

    Ok(r)
}

fn repaint_table(frame: &mut Frame<'_>, area: Rect, data: &mut Data, state: &mut State) {
    let l0 = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Fill(1),
        Constraint::Length(35),
    ])
    .split(area);

    let l1 = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(l0[0]);

    state.edit = layout_edit(
        area,
        &[
            EditConstraint::TitleLabel,
            EditConstraint::Widget(20),
            EditConstraint::Widget(20),
            EditConstraint::Widget(20),
            EditConstraint::Widget(20),
            EditConstraint::Widget(20),
            EditConstraint::Empty,
            EditConstraint::Widget(20),
        ],
    );
    let mut lb = state.edit.iter();

    "rows() reports".render(lb.label(), frame.buffer_mut());
    let mut b_none = Span::from("None").white().on_dark_gray();
    if state.report_rows == None {
        b_none = b_none.on_gray();
    }
    frame.render_widget(b_none, lb.widget());
    let mut b_none = Span::from("Too few").white().on_dark_gray();
    if state.report_rows == Some(99900) {
        b_none = b_none.on_gray();
    }
    frame.render_widget(b_none, lb.widget());
    let mut b_none = Span::from("Circa").white().on_dark_gray();
    if state.report_rows == Some(100000) {
        b_none = b_none.on_gray();
    }
    frame.render_widget(b_none, lb.widget());
    let mut b_none = Span::from("Exact").white().on_dark_gray();
    if state.report_rows == Some(100010) {
        b_none = b_none.on_gray();
    }
    frame.render_widget(b_none, lb.widget());
    let mut b_none = Span::from("Too many").white().on_dark_gray();
    if state.report_rows == Some(100100) {
        b_none = b_none.on_gray();
    }
    frame.render_widget(b_none, lb.widget());

    let goto = Span::from("GOTO 1_000_000").white().on_light_blue();
    frame.render_widget(goto, lb.widget());

    // table
    struct RowIter1<'a> {
        report_rows: Option<usize>,
        iter: Enumerate<Iter<'a, Sample>>,
        item: Option<(usize, &'a Sample)>,
    }

    impl<'a> TableDataIter<'a> for RowIter1<'a> {
        fn rows(&self) -> Option<usize> {
            // None
            // Some(100_000)
            self.report_rows
        }

        fn nth(&mut self, n: usize) -> bool {
            self.item = self.iter.nth(n);
            self.item.is_some()
        }

        fn next(&mut self) -> bool {
            self.item = self.iter.next();
            self.item.is_some()
        }

        fn row_height(&self) -> u16 {
            1
        }

        fn row_style(&self) -> Style {
            Style::default()
        }

        fn render_cell(&self, column: usize, area: Rect, buf: &mut Buffer) {
            let row = self.item.expect("data");
            match column {
                0 => {
                    let row_fmt = NumberFormat::new("000000").expect("fmt");
                    let span = Span::from(row_fmt.fmt_u(row.0));
                    buf.set_style(area, Style::new().black().bg(Color::from_u32(0xe7c787)));
                    span.render(area, buf);
                }
                1 => {
                    let span = Span::from(row.1.text);
                    span.render(area, buf);
                }
                2 => {
                    let num1_fmt = NumberFormat::new("####0.00").expect("fmt");
                    let span = Span::from(num1_fmt.fmt_u(row.1.num1));
                    span.render(area, buf);
                }
                3 => {
                    let num2_fmt = NumberFormat::new("####0.00").expect("fmt");
                    let span = Span::from(num2_fmt.fmt_u(row.1.num2));
                    span.render(area, buf);
                }
                4 => {
                    let cc = if row.1.check { "\u{2622}" } else { "\u{2623}" };
                    let span = Span::from(cc);
                    span.render(area, buf);
                }
                _ => {}
            }
        }
    }

    let mut rr = RowIter1 {
        report_rows: state.report_rows,
        iter: data.table_data.iter().enumerate(),
        item: None,
    };

    let table1 = FTable::default()
        .iter(&mut rr)
        .widths([
            Constraint::Length(6),
            Constraint::Length(20),
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Length(3),
        ])
        .column_spacing(1)
        .header(
            Row::new([
                Cell::from("Nr"),
                Cell::from("Text"),
                Cell::from("Val1"),
                Cell::from("Val2"),
                Cell::from("State"),
            ])
            .style(Style::new().black().bg(Color::from_u32(0x98c379))),
        )
        .footer(
            Row::new(["a", "b", "c", "d", "e"])
                .style(Style::new().black().bg(Color::from_u32(0x98c379))),
        )
        .flex(Flex::End)
        .style(Style::default().bg(Color::Rgb(25, 25, 25)));
    frame.render_stateful_widget(table1, l0[1], &mut state.table);

    render_tablestate(&state.table, l0[2], frame.buffer_mut());
}

fn handle_table(
    event: &crossterm::event::Event,
    _data: &mut Data,
    state: &mut State,
) -> Result<Outcome, anyhow::Error> {
    let r0 = 'f: {
        match event {
            ct_event!(mouse down Left for x,y) => match state.edit.widget_at((*x, *y)) {
                Some(0) => {
                    state.report_rows = None;
                    break 'f Outcome::Changed;
                }
                Some(1) => {
                    state.report_rows = Some(99_900);
                    break 'f Outcome::Changed;
                }
                Some(2) => {
                    state.report_rows = Some(100_000);
                    break 'f Outcome::Changed;
                }
                Some(3) => {
                    state.report_rows = Some(100_010);
                    break 'f Outcome::Changed;
                }
                Some(4) => {
                    state.report_rows = Some(100_100);
                    break 'f Outcome::Changed;
                }
                Some(5) => {
                    state.table.row_offset = 1_000_000;
                    break 'f Outcome::Changed;
                }
                _ => {
                    break 'f Outcome::NotUsed;
                }
            },
            _ => Outcome::NotUsed,
        }
    };

    let r1 = state.table.handle(event, FocusKeys);

    Ok(r0 | r1)
}