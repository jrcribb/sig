use rayon::prelude::*;

use promkit::{async_trait::async_trait, Prompt};
use promkit_core::{
    crossterm::{
        self,
        event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers},
        style::ContentStyle,
    },
    grapheme::StyledGraphemes,
    render::Renderer,
    PaneFactory,
};
use promkit_widgets::{
    listbox::{self, Listbox},
    text_editor,
};

use crate::highlight::highlight;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Index {
    Readline = 0,
    Text = 1,
}

struct Archived {
    renderer: Renderer<Index>,
    readline: text_editor::State,
    // To track changes in the text editor
    prev_query: String,
    // Initial text to search
    init_text: Listbox,
    // Search results
    text: listbox::State,
    highlight_style: ContentStyle,
    case_insensitive: bool,
    cmd: Option<String>,
}

impl Archived {
    fn evaluate_internal(&mut self, event: &Event) -> anyhow::Result<promkit::Signal> {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => {
                if self.cmd.is_some() {
                    // Exiting archive mode here allows
                    // the caller to re-enter streaming mode,
                    // as it is running in an infinite loop.
                    return Ok(promkit::Signal::Quit);
                }
            }

            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => return Err(anyhow::anyhow!("ctrl+c")),

            // Move cursor (text editor)
            Event::Key(KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => {
                self.readline.texteditor.backward();
            }
            Event::Key(KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => {
                self.readline.texteditor.forward();
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => self.readline.texteditor.move_to_head(),
            Event::Key(KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => self.readline.texteditor.move_to_tail(),

            // Move cursor (listbox).
            Event::Key(KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => {
                self.text.listbox.backward();
            }
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => {
                self.text.listbox.forward();
            }

            // Erase char(s).
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => self.readline.texteditor.erase(),
            Event::Key(KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }) => self.readline.texteditor.erase_all(),

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
            }) => match self.readline.edit_mode {
                text_editor::Mode::Insert => self.readline.texteditor.insert(*ch),
                text_editor::Mode::Overwrite => self.readline.texteditor.overwrite(*ch),
            },

            _ => (),
        }
        Ok(promkit::Signal::Continue)
    }
}

#[async_trait]
impl Prompt for Archived {
    async fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn evaluate(&mut self, event: &Event) -> anyhow::Result<promkit::Signal> {
        let signal = self.evaluate_internal(event);
        let (width, height) = crossterm::terminal::size()?;

        let current_query = self.readline.texteditor.text_without_cursor().to_string();
        if self.prev_query != current_query {
            // Update listbox items based on the current query
            self.text.listbox = Listbox::from_styled_graphemes(
                self.init_text
                    .items()
                    .par_iter()
                    .filter_map(|line| {
                        highlight(
                            &current_query,
                            &line.to_string(),
                            self.highlight_style,
                            self.case_insensitive,
                        )
                    })
                    .collect::<Vec<StyledGraphemes>>(),
            );

            // Update previous query
            self.prev_query = current_query;
        }

        // TODO: determine whether to render to check cursor was moved or not
        self.renderer
            .update([
                (Index::Readline, self.readline.create_pane(width, height)),
                (Index::Text, self.text.create_pane(width, height)),
            ])
            .render()
            .await?;

        signal
    }

    type Return = ();

    fn finalize(&mut self) -> anyhow::Result<Self::Return> {
        Ok(())
    }
}

pub async fn run(
    readline: text_editor::State,
    text: listbox::State,
    highlight_style: ContentStyle,
    case_insensitive: bool,
    cmd: Option<String>,
) -> anyhow::Result<()> {
    let (width, height) = crossterm::terminal::size()?;
    Archived {
        renderer: Renderer::try_new_with_panes(
            [
                (Index::Readline, readline.create_pane(width, height)),
                (Index::Text, text.create_pane(width, height)),
            ],
            true,
        )
        .await?,
        prev_query: String::new(),
        readline,
        init_text: text.listbox.clone(),
        text,
        highlight_style,
        case_insensitive,
        cmd,
    }
    .run()
    .await
}
