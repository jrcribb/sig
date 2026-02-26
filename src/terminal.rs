use std::io::{self, Write};

use promkit_core::{
    crossterm::{self, cursor, style, terminal},
    grapheme::StyledGraphemes,
    pane::Pane,
};

pub struct Terminal {
    size: (u16, u16),
    pane_rows: u16,
}

/// Reset the scroll region to the entire terminal.
fn reset_scroll_region_sequence() -> &'static str {
    crossterm::csi!("r")
}

/// Set the scroll region to [top, bottom], where both are 1-based.
fn set_scroll_region_sequence(top_1based: u16, bottom_1based: u16) -> String {
    format!(crossterm::csi!("{};{}r"), top_1based, bottom_1based)
}

impl Terminal {
    /// Create a new Terminal instance and apply the initial scroll region.
    pub fn try_new(size: (u16, u16), pane: &Pane) -> anyhow::Result<Self> {
        let term = Self {
            size,
            pane_rows: Self::pane_rows(size, pane),
        };
        term.apply_scroll_region()?;
        io::stdout().flush()?;
        Ok(term)
    }

    pub fn draw_stream(&mut self, items: &[StyledGraphemes]) -> anyhow::Result<()> {
        let stream_height = self.stream_height();
        if items.is_empty() || stream_height == 0 {
            io::stdout().flush()?;
            return Ok(());
        }

        let visible_rows = items.len().min(stream_height as usize);
        let start = items.len().saturating_sub(visible_rows);
        let rows = &items[start..];
        let scroll_rows = rows.len() as u16;
        let write_from = self.size.1.saturating_sub(scroll_rows);

        crossterm::queue!(
            io::stdout(),
            cursor::MoveTo(0, self.stream_top()),
            terminal::ScrollUp(scroll_rows),
        )?;
        for (idx, row) in rows.iter().enumerate() {
            crossterm::queue!(
                io::stdout(),
                cursor::MoveTo(0, write_from + idx as u16),
                terminal::Clear(terminal::ClearType::CurrentLine),
                style::Print(row.styled_display()),
            )?;
        }

        io::stdout().flush()?;
        Ok(())
    }

    /// Draw the pane content.
    /// This should be called after syncing the layout to ensure the pane area is correctly sized.
    pub fn draw_pane(&mut self, pane: &Pane) -> anyhow::Result<()> {
        for y in 0..self.pane_rows {
            crossterm::queue!(
                io::stdout(),
                cursor::MoveTo(0, y),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }

        for (y, row) in pane.extract(self.pane_rows as usize).iter().enumerate() {
            crossterm::queue!(
                io::stdout(),
                cursor::MoveTo(0, y as u16),
                style::Print(row.styled_display()),
            )?;
        }

        io::stdout().flush()?;
        Ok(())
    }

    pub fn sync_layout(&mut self, size: (u16, u16), pane_rows: u16) -> anyhow::Result<bool> {
        let pane_rows = pane_rows.min(size.1);
        if self.size == size && self.pane_rows == pane_rows {
            return Ok(false);
        }

        self.size = size;
        self.pane_rows = pane_rows;
        self.apply_scroll_region()?;
        self.clear_stream_area()?;
        Ok(true)
    }

    pub fn pane_rows(size: (u16, u16), pane: &Pane) -> u16 {
        (pane.visible_row_count() as u16).min(size.1)
    }

    fn stream_top(&self) -> u16 {
        self.pane_rows
    }

    fn stream_height(&self) -> u16 {
        self.size.1.saturating_sub(self.pane_rows)
    }

    fn clear_stream_area(&self) -> anyhow::Result<()> {
        for y in self.stream_top()..self.size.1 {
            crossterm::queue!(
                io::stdout(),
                cursor::MoveTo(0, y),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }
        Ok(())
    }

    /// Apply the scroll region to the stream area, excluding the pane area.
    fn apply_scroll_region(&self) -> anyhow::Result<()> {
        if self.stream_height() == 0 {
            crossterm::queue!(
                io::stdout(),
                style::Print(reset_scroll_region_sequence()),
            )?;
            return Ok(());
        }

        let top = self.stream_top() + 1;
        let bottom = self.size.1;
        // Exclude the pane area from the scroll region,
        // so that only the stream area is scrolled when new lines are added.
        crossterm::queue!(
            io::stdout(),
            style::Print(set_scroll_region_sequence(top, bottom)),
        )?;
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = crossterm::queue!(io::stdout(), style::Print(reset_scroll_region_sequence()));
        let _ = io::stdout().flush();
    }
}
