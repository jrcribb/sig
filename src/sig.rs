use std::{collections::VecDeque, sync::Arc};

use tokio::{
    sync::{mpsc, watch, RwLock},
    task::JoinHandle,
    time::{self, Duration},
};

use promkit_core::{
    crossterm::{
        self,
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers},
        style::{Color, ContentStyle},
    },
    pane::Pane,
    PaneFactory,
};
use promkit_widgets::{text, text_editor};

use crate::{
    config::{matches_keybind, StreamingKeybinds},
    highlight::highlight,
    spawn,
    terminal::Terminal,
    Signal,
};

enum InputAction {
    Continue,
    TogglePause,
    GotoArchived,
    GotoStreaming,
}

// Evaluate a key event and return the corresponding InputAction.
fn evaluate_event(
    event: &Event,
    state: &mut text_editor::State,
    has_cmd: bool,
    keybinds: &StreamingKeybinds,
) -> anyhow::Result<InputAction> {
    if matches_keybind(event, &keybinds.goto_archived) {
        return Ok(InputAction::GotoArchived);
    }

    if has_cmd && matches_keybind(event, &keybinds.retry) {
        return Ok(InputAction::GotoStreaming);
    }

    if matches_keybind(event, &keybinds.toggle_pause) {
        return Ok(InputAction::TogglePause);
    }

    if matches_keybind(event, &keybinds.exit) {
        return Err(anyhow::anyhow!("exit"));
    }

    if matches_keybind(event, &keybinds.editor.backward) {
        state.texteditor.backward();
        return Ok(InputAction::Continue);
    }

    if matches_keybind(event, &keybinds.editor.forward) {
        state.texteditor.forward();
        return Ok(InputAction::Continue);
    }

    if matches_keybind(event, &keybinds.editor.move_to_head) {
        state.texteditor.move_to_head();
        return Ok(InputAction::Continue);
    }

    if matches_keybind(event, &keybinds.editor.move_to_tail) {
        state.texteditor.move_to_tail();
        return Ok(InputAction::Continue);
    }

    if matches_keybind(event, &keybinds.editor.erase) {
        state.texteditor.erase();
        return Ok(InputAction::Continue);
    }

    if matches_keybind(event, &keybinds.editor.erase_all) {
        state.texteditor.erase_all();
        return Ok(InputAction::Continue);
    }

    match event {
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
    Ok(InputAction::Continue)
}

fn create_panes(text_editor: &text_editor::State, size: (u16, u16), has_cmd: bool) -> Vec<Pane> {
    let retry_hint = if has_cmd { " | Retry" } else { "" };
    let hint = text::State {
        text: text::Text::from(format!("Archived | Pause/Resume{} | Exit", retry_hint)),
        style: ContentStyle {
            foreground_color: Some(Color::DarkGrey),
            ..Default::default()
        },
        lines: Some(1),
    };

    vec![
        text_editor.create_pane(size.0, size.1),
        hint.create_pane(size.0, size.1),
    ]
}

pub async fn run(
    text_editor: text_editor::State,
    highlight_style: ContentStyle,
    keybinds: StreamingKeybinds,
    retrieval_timeout: Duration,
    render_interval: Option<Duration>,
    queue_capacity: usize,
    case_insensitive: bool,
    cmd: Option<String>,
) -> anyhow::Result<(Signal, VecDeque<String>)> {
    let size = crossterm::terminal::size()?;
    let has_cmd = cmd.is_some();

    let panes = create_panes(&text_editor, size, has_cmd);
    let term = Terminal::try_new(size, &panes)?;
    term.draw_pane(&panes)?;

    let shared_term = Arc::new(RwLock::new(term));
    let shared_text_editor = Arc::new(RwLock::new(text_editor));
    let readonly_term = Arc::clone(&shared_term);
    let readonly_text_editor = Arc::clone(&shared_text_editor);
    let (pause_tx, mut pause_rx) = watch::channel(false);

    let (tx, mut rx) = mpsc::channel(1);

    let input_task = match &cmd {
        Some(cmd) => spawn::spawn_cmd_result_sender(cmd, tx, retrieval_timeout),
        None => spawn::spawn_stdin_sender(tx, retrieval_timeout),
    }?;

    let keeping: JoinHandle<anyhow::Result<VecDeque<String>>> = tokio::spawn(async move {
        let mut queue = VecDeque::with_capacity(queue_capacity);
        let mut maybe_interval = render_interval.map(|p| time::interval(p));
        let mut paused = false;

        loop {
            // While paused:
            // - Keep watching pause state changes so resume is immediate.
            // - Keep watching the input channel to detect EOF and terminate cleanly.
            //   Incoming lines are intentionally dropped while paused.
            if paused {
                tokio::select! {
                    biased;
                    changed = pause_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        paused = *pause_rx.borrow_and_update();
                    }
                    maybe_line = rx.recv() => {
                        // Even while paused, observe EOF so the task can terminate.
                        if maybe_line.is_none() {
                            break;
                        }
                        // Ignore incoming lines while paused.
                    }
                }
                continue;
            }

            // When render throttling is enabled:
            // - Wait for the next render tick before processing input.
            // - Allow pause changes to interrupt the wait so Ctrl+S stays responsive.
            if let Some(interval) = &mut maybe_interval {
                tokio::select! {
                    biased;
                    changed = pause_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        paused = *pause_rx.borrow_and_update();
                        continue;
                    }
                    _ = interval.tick() => {
                        // Proceed to input handling after the interval tick.
                    }
                }
            }

            tokio::select! {
                biased;
                changed = pause_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    paused = *pause_rx.borrow_and_update();
                }
                maybe_line = rx.recv() => {
                    match maybe_line {
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
                                let panes = create_panes(&text_editor, size, has_cmd);
                                let mut term = readonly_term.write().await;
                                let pane_rows = Terminal::pane_rows(size, &panes);
                                if term.sync_layout(size, pane_rows)? {
                                    term.draw_pane(&panes)?;
                                }
                                term.draw_stream(&matrix)?;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(queue)
    });

    let mut paused = false;
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
        let action = evaluate_event(&event, &mut text_editor, has_cmd, &keybinds)?;
        match action {
            InputAction::GotoArchived => break Signal::GotoArchived,
            InputAction::GotoStreaming => break Signal::GotoStreaming,
            InputAction::TogglePause => {
                paused = !paused;
                let _ = pause_tx.send(paused);
            }
            InputAction::Continue => {}
        }

        let size = crossterm::terminal::size()?;
        let panes = create_panes(&text_editor, size, has_cmd);
        let mut term = shared_term.write().await;
        term.sync_layout(size, Terminal::pane_rows(size, &panes))?;
        term.draw_pane(&panes)?;
    };

    if let Some(mut child) = input_task.child {
        let _ = child.kill().await;
    }
    input_task.handle.abort();

    Ok((signal, keeping.await??))
}
