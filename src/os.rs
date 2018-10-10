pub mod unix {
    extern crate libc;

    use std::error::Error;
    use std::ffi::CStr;
    use std::fmt;
    use std::mem;

    #[derive(Debug)]
    pub struct SysError(i32, String);

    impl SysError {
        fn last(s: &str) -> SysError {
            unsafe {
                let err_code = errno();
                let err_string = libc::strerror(err_code);
                SysError(
                    err_code,
                    s.to_string() + ": " + &CStr::from_ptr(err_string).to_string_lossy(),
                )
            }
        }
    }
    impl fmt::Display for SysError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} (code: {})", self.1, self.0)
        }
    }

    impl Error for SysError {}

    fn errno() -> i32 {
        unsafe { *libc::__errno_location() }
    }

    pub fn read() -> Result<u8, SysError> {
        let mut c = unsafe { mem::uninitialized() };
        let res =
            unsafe { libc::read(libc::STDIN_FILENO, &mut c as *mut _ as *mut libc::c_void, 1) };
        match res {
            1 => Ok(c),
            0 => Ok(0),
            -1 if errno() == libc::EAGAIN => Ok(0),
            _ => Err(SysError::last("read")),
        }
    }

    pub fn write(buf: &str) -> Result<(), SysError> {
        let res = unsafe { libc::write(libc::STDOUT_FILENO, buf.as_ptr() as *const _, buf.len()) };
        match res {
            -1 => Err(SysError::last("write")),
            _ => Ok(()),
        }
    }

    fn tcgetattr() -> Result<libc::termios, SysError> {
        let mut termios = unsafe { mem::uninitialized() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) } == 0 {
            Ok(termios)
        } else {
            Err(SysError::last("tcgetattr"))
        }
    }

    fn tcsetattr(termios: &libc::termios) -> Result<(), SysError> {
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios) } == 0 {
            Ok(())
        } else {
            Err(SysError::last("tcsetattr"))
        }
    }

    pub struct OriginalTerminalAttributes {
        attr: libc::termios,
    }

    impl Drop for OriginalTerminalAttributes {
        fn drop(&mut self) {
            tcsetattr(&self.attr).unwrap();
        }
    }

    pub fn enable_raw_mode() -> Result<OriginalTerminalAttributes, SysError> {
        let attr = tcgetattr()?;

        let mut raw = attr;

        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
        raw.c_oflag &= !libc::OPOST;
        raw.c_cflag |= libc::CS8;
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 1;

        tcsetattr(&raw)?;

        Ok(OriginalTerminalAttributes { attr: attr })
    }

}
#[cfg(unix)]
pub use self::unix as target;
