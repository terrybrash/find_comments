pub(crate) const SMALL_FILE: usize = 64 * 1024;

#[cfg(unix)]
pub(crate) fn with_file<R>(
    path: *const u8,
    size: usize,
    buffer: &mut Vec<u8>,
    read_all: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    use std::ffi::{c_char, c_int, c_void};

    const O_RDONLY: c_int = 0;
    const SEEK_END: c_int = 2;
    const PROT_READ: c_int = 1;
    const MAP_PRIVATE: c_int = 2;

    unsafe extern "C" {
        fn open(path: *const c_char, flags: c_int, ...) -> c_int;
        fn close(fd: c_int) -> c_int;
        fn read(fd: c_int, into: *mut c_void, count: usize) -> isize;
        fn lseek(fd: c_int, offset: i64, whence: c_int) -> i64;
        fn mmap(
            at: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(at: *mut c_void, len: usize) -> c_int;
    }

    let fd = unsafe { open(path.cast::<c_char>(), O_RDONLY) };
    if fd < 0 {
        return None;
    }

    let capacity = buffer.capacity();
    if size <= capacity {
        let mut filled = 0;
        while filled < capacity {
            let got = unsafe {
                read(fd, buffer.as_mut_ptr().add(filled).cast::<c_void>(), capacity - filled)
            };
            if got <= 0 {
                break;
            }
            filled += got as usize;
        }
        if filled < capacity {
            unsafe {
                close(fd);
                buffer.set_len(filled);
            }
            return Some(read_all(buffer));
        }
    }

    let len = unsafe { lseek(fd, 0, SEEK_END) };
    if len <= 0 {
        unsafe { close(fd) };
        return None;
    }
    let len = len as usize;
    let at = unsafe { mmap(std::ptr::null_mut(), len, PROT_READ, MAP_PRIVATE, fd, 0) };
    unsafe { close(fd) };
    if at as isize == -1 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(at.cast::<u8>(), len) };
    let out = read_all(bytes);
    unsafe { munmap(at, len) };
    Some(out)
}

#[cfg(not(unix))]
pub(crate) fn with_file<R>(
    path: *const u8,
    _size: usize,
    buffer: &mut Vec<u8>,
    read_all: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    use std::io::Read;

    let bytes = unsafe { std::ffi::CStr::from_ptr(path.cast()) };
    let text = bytes.to_str().ok()?;
    let mut handle = std::fs::File::open(text).ok()?;
    buffer.clear();
    handle.read_to_end(buffer).ok()?;
    Some(read_all(buffer))
}
