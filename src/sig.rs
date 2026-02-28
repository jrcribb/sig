use std::{collections::VecDeque, sync::Arc};

use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
    time::{self, Duration},
};

use promkit_core::{
    crossterm::{
        self,
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers},
        style::ContentStyle,
    },
    PaneFactory,
};
use promkit_widgets::text_editor;

use crate::{highlight::highlight, spawn, terminal::Terminal, Signal};

// Evaluate a key event and return the corresponding Signal.
fn evaluate_event(
    event: &Event,
    state: &mut text_editor::State,
    cmd: Option<String>,
) -> anyhow::Result<Signal> {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => return Ok(Signal::GotoArchived),

        Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            if cmd.is_some() {
                return Ok(Signal::GotoStreaming);
            }
        }

        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => return Err(anyhow::anyhow!("ctrl+c")),

        // Move cursor.
        Event::Key(KeyEvent {
            code: KeyCode::Left,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            state.texteditor.backward();
        }
        Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            state.texteditor.forward();
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => state.texteditor.move_to_head(),
        Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => state.texteditor.move_to_tail(),

        // Erase char(s).
        Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => state.texteditor.erase(),
        Event::Key(KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => state.texteditor.erase_all(),

        // Input char.
        Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
        | Event::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => match state.edit_mode {
            text_editor::Mode::Insert => state.texteditor.insert(*ch),
            text_editor::Mode::Overwrite => state.texteditor.overwrite(*ch),
        },

        _ => (),
    }
    Ok(Signal::Continue)
}

pub async fn run(
    text_editor: text_editor::State,
    highlight_style: ContentStyle,
    retrieval_timeout: Duration,
    render_interval: Option<Duration>,
    queue_capacity: usize,
    case_insensitive: bool,
    cmd: Option<String>,
) -> anyhow::Result<(Signal, VecDeque<String>)> {
    let size = crossterm::terminal::size()?;

    let pane = text_editor.create_pane(size.0, size.1);
    let mut term = Terminal::new(size, &pane)?;
    term.draw_pane(&pane)?;

    let shared_term = Arc::new(RwLock::new(term));
    let shared_text_editor = Arc::new(RwLock::new(text_editor));
    let readonly_term = Arc::clone(&shared_term);
    let readonly_text_editor = Arc::clone(&shared_text_editor);

    let (tx, mut rx) = mpsc::channel(1);

    let input_task = match &cmd {
        Some(cmd) => spawn::spawn_cmd_result_sender(cmd, tx, retrieval_timeout),
        None => spawn::spawn_stdin_sender(tx, retrieval_timeout),
    }?;

    let keeping: JoinHandle<anyhow::Result<VecDeque<String>>> = tokio::spawn(async move {
        let mut queue = VecDeque::with_capacity(queue_capacity);
        let mut maybe_interval = render_interval.map(|p| time::interval(p));

        loop {
            if let Some(interval) = &mut maybe_interval {
                interval.tick().await;
            }
            match rx.recv().await {
                Some(line) => {
                    let text_editor = readonly_text_editor.read().await;
                    let size = crossterm::terminal::size()?;

                    if queue.len() > queue_capacity {
                        queue.pop_front().unwrap();
                    }
                    queue.push_back(line.clone());

                    if let Some(highlighted) = highlight(
                        &text_editor.texteditor.text_without_cursor().to_string(),
                        &line,
                        highlight_style,
                        case_insensitive,
                    ) {
                        let matrix = highlighted.matrixify(size.0 as usize, size.1 as usize, 0).0;
                        let pane = text_editor.create_pane(size.0, size.1);
                        let mut term = readonly_term.write().await;
                        let pane_rows = Terminal::pane_rows(size, &pane);
                        if term.sync_layout(size, pane_rows)? {
                            term.draw_pane(&pane)?;
                        }
                        term.draw_stream(&matrix)?;
                    }
                }
                None => break,
            }
        }
        Ok(queue)
    });

    let signal = loop {
        // Treat an exhausted input source as archived data.
        if keeping.is_finished() {
            break Signal::GotoArchived;
        }

        if !event::poll(retrieval_timeout)? {
            continue;
        }

        let event = event::read()?;
        let mut text_editor = shared_text_editor.write().await;
        let signal = evaluate_event(&event, &mut text_editor, cmd.clone())?;
        if signal == Signal::GotoArchived || signal == Signal::GotoStreaming {
            break signal;
        }

        let size = crossterm::terminal::size()?;
        let pane = text_editor.create_pane(size.0, size.1);
        let mut term = shared_term.write().await;
        term.sync_layout(size, Terminal::pane_rows(size, &pane))?;
        term.draw_pane(&pane)?;
    };

    if let Some(mut child) = input_task.child {
        let _ = child.kill().await;
    }
    input_task.handle.abort();

    Ok((signal, keeping.await??))
}
