use std::io;

mod os;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const CTRL: u8 = 0x1f;
const CTRL_Q: u8 = CTRL & b'q';

struct Editor {
    screen_rows: u16,
    screen_cols: u16,
    term: os::target::Terminal,
}

impl Editor {
    fn new() -> Result<Editor, io::Error> {
        let term = os::target::Terminal::new_raw_mode()?;
        let (rows, cols) = term.get_window_size()?;
        Ok(Editor {
            screen_rows: rows,
            screen_cols: cols,
            term: term,
        })
    }

    fn draw_rows(&mut self) {
        for y in 0..self.screen_rows {
            if y == self.screen_rows / 3 {
                let welcome = format!("Kilo editor -- version {}", VERSION);
                let len = welcome.len().min(self.screen_cols as usize);
                let mut padding = (self.screen_cols as usize - len) / 2;
                if padding > 0 {
                    self.term.push('~');
                    padding -= 1;
                }
                for _ in 0..padding {
                    self.term.push(' ');
                }
                self.term.push_str(&welcome[..len]);
            } else {
                self.term.push('~');
            }
            self.term.erase_in_line();
            if y < self.screen_rows - 1 {
                self.term.push_str("\r\n");
            }
        }
    }

    fn refresh_screen(&mut self) -> Result<(), io::Error> {
        self.term.begin();

        self.term.hide_cursor();
        self.term.move_cursor();

        self.draw_rows();

        self.term.move_cursor();
        self.term.show_cursor();

        self.term.end()?;
        Ok(())
    }

    fn run(&mut self) -> Result<(), io::Error> {
        loop {
            self.refresh_screen()?;
            match os::target::read()? {
                Some(CTRL_Q) => {
                    self.term.begin();
                    self.term.erase_in_display();
                    self.term.move_cursor();
                    self.term.end()?;
                    return Ok(());
                }
                _ => (),
            }
        }
    }
}

fn main() -> Result<(), io::Error> {
    Editor::new()?.run()
}
