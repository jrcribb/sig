use std::{io, io::Write, path::PathBuf};

use anyhow::anyhow;
use clap::Parser;
use tokio::time::Duration;

use promkit_core::crossterm::{
    self, cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use promkit_widgets::{
    listbox,
    text_editor::{self, TextEditor},
};

mod archived;
mod config;
mod highlight;
mod sig;
mod spawn;
mod terminal;
use config::{Config, DEFAULT_CONFIG};

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

Static input (switches to archived view after EOF):
$ cat README.md |& sig

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

    #[arg(short = 'c', long = "config", help = "Path to the configuration file.")]
    pub config_file: Option<PathBuf>,
}

impl Drop for Args {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), DisableMouseCapture, cursor::Show).ok();
    }
}

/// Ensure that the specified file exists.
/// If it does not exist, creates the file and its parent directories if necessary,
/// and writes the default configuration content to it.
fn ensure_file_exists(path: &PathBuf) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| anyhow!("Failed to create directory: {e}"))?;
    }

    std::fs::File::create(path)?.write_all(DEFAULT_CONFIG.as_bytes())?;
    Ok(())
}

/// Determine the configuration file path.
fn determine_config_file(config_path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = config_path {
        ensure_file_exists(&path)?;
        return Ok(path);
    }

    let default_path = dirs::config_dir()
        .ok_or_else(|| anyhow!("Failed to determine the configuration directory"))?
        .join("sig")
        .join("config.toml");

    ensure_file_exists(&default_path)?;
    Ok(default_path)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = determine_config_file(args.config_file.clone())
        .and_then(|config_file| {
            std::fs::read_to_string(&config_file)
                .map_err(|e| anyhow!("Failed to read configuration file: {e}"))
        })
        .and_then(|content| Config::load_from(&content))
        .unwrap_or_else(|_e| {
            Config::load_from(DEFAULT_CONFIG).expect("Failed to load default configuration")
        });

    enable_raw_mode()?;
    execute!(io::stdout(), cursor::Hide)?;

    while let Ok((signal, queue)) = sig::run(
        text_editor::State {
            texteditor: TextEditor::new(args.query.clone().unwrap_or_default()),
            history: Default::default(),
            config: config.streaming.editor.clone(),
        },
        config.highlight_style,
        config.streaming.keybinds.clone(),
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
                execute!(io::stdout(), EnableMouseCapture)?;

                archived::run(
                    text_editor::State {
                        texteditor: TextEditor::new(String::new()),
                        history: Default::default(),
                        config: config.archived.editor.clone(),
                    },
                    listbox::State {
                        listbox: listbox::Listbox::from(queue),
                        config: config.archived.listbox.clone(),
                    },
                    config.highlight_style,
                    config.archived.keybinds.clone(),
                    args.case_insensitive,
                    args.cmd.clone(),
                )
                .await?;

                // Re-enable raw mode and hide the cursor again here
                // because they are disabled and shown, respectively, by promkit.
                enable_raw_mode()?;
                execute!(io::stdout(), DisableMouseCapture, cursor::Hide)?;

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

    Ok(())
}
