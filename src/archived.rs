use rayon::prelude::*;

use promkit::{Prompt, Signal, async_trait::async_trait};
use promkit_core::{
    PaneFactory, crossterm::{self, event::Event, style::ContentStyle}, grapheme::StyledGraphemes, pane::Pane, render::Renderer
};
use promkit_widgets::{listbox, text_editor};

use crate::sig;

mod keymap;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Index {
    Readline = 0,
    Logs = 1,
}

struct Archived {
    renderer: Renderer<Index>,
    readline: text_editor::State,
    // To track changes in the text editor
    prev_query: String,
    logs: listbox::State,
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
        let signal = keymap::default(event, &mut self.readline, &mut self.logs, self.cmd.clone());
        let (width, height) = crossterm::terminal::size()?;
        // TODO: check diff about 
        self.renderer
            .update([(Index::Readline, self.readline.create_pane(width, height))]);

        let current_query = self.readline.texteditor.text_without_cursor().to_string();
        if self.prev_query != current_query {

        let list: Vec<StyledGraphemes> = self
            .logs
            .listbox
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
            .collect();

            // TODO: use list with listbox::State
            self.renderer.update([]);

            // Update previous query
            self.prev_query = current_query;
        }
        
        // Render the updated panes
        self.renderer.render().await?;

        signal
    }

    type Return = ();

    fn finalize(&mut self) -> anyhow::Result<Self::Return> {
        Ok(())
    }
}

pub async fn run(
    readline: text_editor::State,
    logs: listbox::State,
    highlight_style: ContentStyle,
    case_insensitive: bool,
    cmd: Option<String>,
) -> anyhow::Result<()> {
    let (width, height) = crossterm::terminal::size()?;
    Archived {
        renderer: Renderer::try_new_with_panes(
            [
                (Index::Readline, readline.create_pane(width, height)),
                (Index::Logs, logs.create_pane(width, height))
            ],
            true,
        ).await?,
        prev_query: String::new(),
        readline,
        logs,
        highlight_style,
        case_insensitive,
        cmd,
    }.run().await
}
