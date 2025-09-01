use rayon::prelude::*;

use promkit::{async_trait::async_trait, Prompt, Signal};
use promkit_core::{
    crossterm::{self, event::Event, style::ContentStyle},
    grapheme::StyledGraphemes,
    render::Renderer,
    PaneFactory,
};
use promkit_widgets::{listbox::{self, Listbox}, text_editor};

use crate::sig;

mod keymap;

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

#[async_trait]
impl Prompt for Archived {
    async fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn evaluate(&mut self, event: &Event) -> anyhow::Result<Signal> {
        let signal = keymap::default(event, &mut self.readline, &mut self.text, self.cmd.clone());
        let (width, height) = crossterm::terminal::size()?;

        let current_query = self.readline.texteditor.text_without_cursor().to_string();
        if self.prev_query != current_query {
            // Update listbox items based on the current query
            self.text.listbox = Listbox::from_styled_graphemes(
                self
                    .init_text
                    .items()
                    .par_iter()
                    .filter_map(|line| {
                        sig::styled(
                            &current_query,
                            &line.to_string(),
                            self.highlight_style,
                            self.case_insensitive,
                        )
                    })
                    .collect::<Vec<StyledGraphemes>>()
            );

            // Update previous query
            self.prev_query = current_query;
        }

        // TODO: determine whether to render to check cursor was moved or not
        self.renderer
            .update([
                (Index::Readline, self.readline.create_pane(width, height)),
                (Index::Text, self.text.create_pane(width, height)),
            ]).render().await?;

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
