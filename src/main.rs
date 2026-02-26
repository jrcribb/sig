use std::{collections::VecDeque, io};

use clap::Parser;
use tokio::{
    sync::mpsc,
    time::{timeout, Duration},
};

use promkit_core::crossterm::{
    self, cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::{Color, ContentStyle},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use promkit_widgets::{
    listbox,
    text_editor::{self, TextEditor},
};

mod archived;
mod highlight;
mod sig;
mod spawn;
mod terminal;

#[derive(Eq, PartialEq)]
pub enum Signal {
    Continue,
    GotoArchived,
    GotoStreaming,
}

/// Interactive grep (for streaming)
#[derive(Parser)]
#[command(
    name = "sig",
    version,
    help_template = "
{about}

Usage: {usage}

Examples:

$ stern --context kind-kind etcd |& sig
Or the method to retry command by pressing ctrl+r:
$ sig --cmd \"stern --context kind-kind etcd\"

Archived mode:
$ cat README.md |& sig -a
Or
$ sig -a --cmd \"cat README.md\"

Options:
{options}
"
)]
pub struct Args {
    #[arg(
        long = "retrieval-timeout",
        default_value = "10",
        help = "Timeout to read a next line from the stream in milliseconds."
    )]
    pub retrieval_timeout_millis: u64,

    #[arg(
        long = "render-interval",
        default_value = None,
        help = "Interval to render a line in milliseconds.",
        long_help = "Adjust this value to prevent screen flickering
        when a large volume of lines is rendered in a short period."
    )]
    pub render_interval_millis: Option<u64>,

    #[arg(
        short = 'q',
        long = "queue-capacity",
        default_value = "1000",
        help = "Queue capacity to store lines.",
        long_help = "Queue capacity for storing lines.
        This value is used for temporary storage of lines
        and should be adjusted based on the system's memory capacity.
        Increasing this value allows for more lines to be stored temporarily,
        which can be beneficial when digging deeper into lines with the digger."
    )]
    pub queue_capacity: usize,

    #[arg(
        short = 'a',
        long = "archived",
        default_value = "false",
        help = "Archived mode to grep through static data."
    )]
    pub archived: bool,

    #[arg(
        short = 'i',
        long = "ignore-case",
        default_value = "false",
        help = "Case insensitive search."
    )]
    pub case_insensitive: bool,

    #[arg(
        long = "cmd",
        help = "Command to execute on initial and retries.",
        long_help = "This command is invoked initially and
        whenever a retry is triggered according to key mappings."
    )]
    pub cmd: Option<String>,

    #[arg(
        short = 'Q',
        long = "query",
        help = "Initial query.",
        long_help = "This query is set as the initial text
        in the text editor when the program starts."
    )]
    pub query: Option<String>,
}

impl Drop for Args {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), DisableMouseCapture, cursor::Show).ok();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    enable_raw_mode()?;
    execute!(io::stdout(), EnableMouseCapture, cursor::Hide)?;

    let highlight_style = ContentStyle {
        foreground_color: Some(Color::Red),
        ..Default::default()
    };

    if args.archived {
        let (tx, mut rx) = mpsc::channel(1);

        let input_task = match &args.cmd {
            Some(cmd) => spawn::spawn_cmd_result_sender(
                cmd,
                tx,
                Duration::from_millis(args.retrieval_timeout_millis),
            ),
            None => {
                spawn::spawn_stdin_sender(tx, Duration::from_millis(args.retrieval_timeout_millis))
            }
        }?;

        let mut queue = VecDeque::with_capacity(args.queue_capacity);
        loop {
            match timeout(
                Duration::from_millis(args.retrieval_timeout_millis),
                rx.recv(),
            )
            .await
            {
                Ok(Some(line)) => {
                    if queue.len() > args.queue_capacity {
                        queue.pop_front().unwrap();
                    }
                    queue.push_back(line.clone());
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // Stop the input task
        input_task.handle.abort();

        crossterm::execute!(
            io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            cursor::MoveTo(0, 0),
        )?;

        archived::run(
            text_editor::State {
                texteditor: TextEditor::new(args.query.clone().unwrap_or_default()),
                prefix: String::from("❯❯❯ "),
                prefix_style: ContentStyle {
                    foreground_color: Some(Color::DarkBlue),
                    ..Default::default()
                },
                active_char_style: ContentStyle {
                    background_color: Some(Color::DarkCyan),
                    ..Default::default()
                },
                ..Default::default()
            },
            listbox::State {
                listbox: listbox::Listbox::from_displayable(queue),
                cursor: String::from("❯ "),
                active_item_style: None,
                inactive_item_style: None,
                lines: Default::default(),
            },
            highlight_style,
            args.case_insensitive,
            // In archived mode, command for retry is meaningless.
            None,
        )
        .await?;
    } else {
        while let Ok((signal, queue)) = sig::run(
            text_editor::State {
                texteditor: TextEditor::new(args.query.clone().unwrap_or_default()),
                prefix: String::from("❯❯ "),
                prefix_style: ContentStyle {
                    foreground_color: Some(Color::DarkGreen),
                    ..Default::default()
                },
                active_char_style: ContentStyle {
                    background_color: Some(Color::DarkCyan),
                    ..Default::default()
                },
                ..Default::default()
            },
            highlight_style,
            Duration::from_millis(args.retrieval_timeout_millis),
            args.render_interval_millis.map(Duration::from_millis),
            args.queue_capacity,
            args.case_insensitive,
            args.cmd.clone(),
        )
        .await
        {
            crossterm::execute!(
                io::stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                cursor::MoveTo(0, 0),
            )?;

            match signal {
                Signal::GotoArchived => {
                    archived::run(
                        text_editor::State {
                            prefix: String::from("❯❯❯ "),
                            prefix_style: ContentStyle {
                                foreground_color: Some(Color::DarkBlue),
                                ..Default::default()
                            },
                            active_char_style: ContentStyle {
                                background_color: Some(Color::DarkCyan),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        listbox::State {
                            listbox: listbox::Listbox::from_displayable(queue),
                            cursor: String::from("❯ "),
                            active_item_style: None,
                            inactive_item_style: None,
                            lines: Default::default(),
                        },
                        highlight_style,
                        args.case_insensitive,
                        args.cmd.clone(),
                    )
                    .await?;

                    // Re-enable raw mode and hide the cursor again here
                    // because they are disabled and shown, respectively, by promkit.
                    enable_raw_mode()?;
                    execute!(io::stdout(), EnableMouseCapture, cursor::Hide)?;

                    crossterm::execute!(
                        io::stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        cursor::MoveTo(0, 0),
                    )?;
                }
                Signal::GotoStreaming => {
                    continue;
                }
                _ => {}
            }
        }
    }

    Ok(())
}
