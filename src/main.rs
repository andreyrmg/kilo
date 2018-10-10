use std::error::Error;

mod os;

const CTRL: u8 = 0x1f;
const CTRL_Q: u8 = CTRL & b'q';

#[macro_use]
mod vt100 {
    macro_rules! write_csi {
        ($cmd:expr) => {
            os::target::write(concat!("\x1b[", $cmd))?
        };
    }
    macro_rules! erase_in_display {
        (all) => {
            write_csi!("2J")
        };
    }
    macro_rules! cursor_position {
        () => {
            write_csi!("H")
        };
    }
}

struct Editor {
    _orig_term_attr: os::target::OriginalTerminalAttributes,
}

impl Editor {
    fn new() -> Result<Editor, Box<Error>> {
        Ok(Editor {
            _orig_term_attr: os::target::enable_raw_mode()?,
        })
    }
    fn refresh_screen(&self) -> Result<(), Box<Error>> {
        erase_in_display!(all);
        cursor_position!();
        Ok(())
    }
    fn run(&mut self) -> Result<(), Box<Error>> {
        loop {
            self.refresh_screen()?;
            match os::target::read()? {
                CTRL_Q => {
                    erase_in_display!(all);
                    cursor_position!();
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
