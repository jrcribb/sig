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

impl Terminal {
    pub fn new(size: (u16, u16), pane: &Pane) -> anyhow::Result<Self> {
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

    pub fn draw_pane(&mut self, pane: &Pane) -> anyhow::Result<()> {
        self.redraw_pane_rows(pane)?;
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

    fn redraw_pane_rows(&self, pane: &Pane) -> anyhow::Result<()> {
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
        Ok(())
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

    fn apply_scroll_region(&self) -> anyhow::Result<()> {
        if self.stream_height() == 0 {
            crossterm::queue!(io::stdout(), style::Print("\x1b[r"))?;
            return Ok(());
        }

        let top = self.stream_top() + 1;
        let bottom = self.size.1;
        crossterm::queue!(io::stdout(), style::Print(format!("\x1b[{top};{bottom}r")))?;
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = crossterm::queue!(io::stdout(), style::Print("\x1b[r"));
        let _ = io::stdout().flush();
    }
}
