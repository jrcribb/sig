use std::fmt;

use promkit_core::crossterm::Command;

/// Enable xterm alternate scroll mode (`CSI ? 1007 h`).
///
/// This avoids capturing click events while allowing wheel input to be
/// translated into cursor up/down on the alternate screen.
///
/// NOTE:
/// This mode is intended to be used together with
/// `crossterm::terminal::EnterAlternateScreen` (`CSI ? 1049 h`).
/// In the normal screen buffer, terminal scrollback usually takes
/// precedence and wheel input may not be forwarded to the application.
///
/// References:
/// - https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnableAlternateScrollCapture;

impl Command for EnableAlternateScrollCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(concat!(
            // Reset all related modes first.
            "\x1b[?1007l",
            "\x1b[?1016l",
            "\x1b[?1006l",
            "\x1b[?1015l",
            "\x1b[?1003l",
            "\x1b[?1002l",
            "\x1b[?1000l",
            // Enable alternate scroll mode only.
            "\x1b[?1007h",
        ))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisableAlternateScrollCapture;

impl Command for DisableAlternateScrollCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(concat!(
            "\x1b[?1007l",
            "\x1b[?1016l",
            "\x1b[?1006l",
            "\x1b[?1015l",
            "\x1b[?1003l",
            "\x1b[?1002l",
            "\x1b[?1000l",
        ))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Ok(())
    }
}
