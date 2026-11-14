use std::{
    ffi::CString,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

pub(super) struct Mapping {
    address: *mut u8,
    len: usize,
}

// Readers use Avo's published counters and copy out of this read-only mapping.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Mapping {
    pub(super) fn open(name: &str) -> io::Result<Self> {
        let name = CString::new(name)?;
        let fd = unsafe { libc::shm_open(name.as_ptr(), libc::O_RDONLY) };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let len = usize::try_from(file.metadata()?.len()).map_err(io::Error::other)?;
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            address: address.cast(),
            len,
        })
    }

    fn pointer(&self, offset: usize, length: usize) -> *const u8 {
        assert!(offset <= self.len && length <= self.len - offset);
        self.address.wrapping_add(offset)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn word(&self, offset: usize) -> u32 {
        let word = self.pointer(offset, 4).cast::<AtomicU32>();
        assert!(word.is_aligned());
        unsafe { (*word).load(Ordering::Acquire) }.to_le()
    }

    pub(super) fn sequence(&self, offset: usize) -> u64 {
        let word = self.pointer(offset, 8).cast::<AtomicU64>();
        assert!(word.is_aligned());
        unsafe { (*word).load(Ordering::Acquire) }.to_le()
    }

    pub(super) fn copy(&self, offset: usize, output: &mut [u8]) {
        let address = self.pointer(offset, output.len());
        unsafe { std::ptr::copy_nonoverlapping(address, output.as_mut_ptr(), output.len()) };
        std::sync::atomic::fence(Ordering::AcqRel);
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.address.cast(), self.len) };
    }
}
