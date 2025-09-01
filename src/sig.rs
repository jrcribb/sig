use std::{collections::VecDeque, sync::Arc};

use tokio::{
    sync::{mpsc, RwLock},
    task::JoinHandle,
    time::{self, Duration},
};

use promkit_core::{
    crossterm::{self, event, style::ContentStyle},
    PaneFactory,
};
use promkit_widgets::text_editor;

mod keymap;
use crate::{highlight::highlight, spawn, terminal::Terminal, Signal};

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
    let mut term = Terminal::new(&pane)?;
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
                        let term = readonly_term.read().await;
                        term.draw_stream_and_pane(
                            matrix,
                            &text_editor.create_pane(size.0, size.1),
                        )?;
                    }
                }
                None => break,
            }
        }
        Ok(queue)
    });

    let mut signal: Signal;
    loop {
        let event = event::read()?;
        let mut text_editor = shared_text_editor.write().await;
        signal = keymap::default(&event, &mut text_editor, cmd.clone())?;
        if signal == Signal::GotoArchived || signal == Signal::GotoStreaming {
            break;
        }

        let size = crossterm::terminal::size()?;
        let pane = text_editor.create_pane(size.0, size.1);
        let mut term = shared_term.write().await;
        term.draw_pane(&pane)?;
    }

    if let Some(mut child) = input_task.child {
        let _ = child.kill().await;
    }
    input_task.handle.abort();

    Ok((signal, keeping.await??))
}
