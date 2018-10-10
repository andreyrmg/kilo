use std::error::Error;

mod os;

const CTRL: u8 = 0x1f;
const CTRL_Q: u8 = CTRL & b'q';

struct Editor {
    screen_rows: u16,
    screen_cols: u16,
    term: os::target::Terminal,
}

impl Editor {
    fn new() -> Result<Editor, Box<Error>> {
        let term = os::target::Terminal::new_raw_mode()?;
        let (rows, cols) = term.get_window_size()?;
        Ok(Editor {
            screen_rows: rows,
            screen_cols: cols,
            term: term,
        })
    }

    fn draw_rows(&mut self) {
        for i in 0..self.screen_rows {
            self.term.push("~");
            self.term.erase_in_line();
            if i < self.screen_rows - 1 {
                self.term.push("\r\n");
            }
        }
    }

    fn refresh_screen(&mut self) -> Result<(), Box<Error>> {
        self.term.begin();

        self.term.hide_cursor();
        self.term.move_cursor();

        self.draw_rows();

        self.term.move_cursor();
        self.term.show_cursor();

        self.term.end()?;
        Ok(())
    }

    fn run(&mut self) -> Result<(), Box<Error>> {
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

fn main() -> Result<(), Box<Error>> {
    Editor::new()?.run()
}
