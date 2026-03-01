use std::fmt;

use promkit_core::crossterm::Command;

/// Enable xterm alternate scroll mode (`CSI ? 1007 h`).
///
/// This avoids capturing click events while allowing wheel input to be
/// translated into cursor up/down on the alternate screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnableMouseScrollCapture;

impl Command for EnableMouseScrollCapture {
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
pub struct DisableMouseScrollCapture;

impl Command for DisableMouseScrollCapture {
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
