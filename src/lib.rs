#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use std::{
    fmt::Arguments,
    fs::File,
    io,
    io::{IoSlice, IoSliceMut, SeekFrom},
    ops::{Deref, DerefMut},
};

mutually_exclusive_features::exactly_one_of! {
    "backend-async-std",
    "backend-rayon",
    "backend-smol",
    "backend-threadpool",
}

#[cfg(not(windows))]
pub use stub::FastClose;
#[cfg(windows)]
pub use windows::FastClose;

pub mod fs;

/// The Windows implementation of [`FastClose`]
#[cfg(windows)]
mod windows {
    #[cfg(feature = "backend-threadpool")]
    use std::sync::OnceLock;
    use std::{
        fmt, io, mem::ManuallyDrop, ops::Deref, os::windows::prelude::*,
    };

    #[cfg(feature = "backend-threadpool")]
    use threadpool::{Builder as ThreadPoolBuilder, ThreadPool};

    use crate::FastCloseable;

    /// A lazily initialised [`ThreadPool`] to send handle closures to
    #[cfg(feature = "backend-threadpool")]
    static CLOSER_POOL: OnceLock<ThreadPool> = OnceLock::new();

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    pub struct FastClose<H: Send + 'static>(pub(super) ManuallyDrop<H>);

    // Public interface
    impl<H: FastCloseable> FastClose<H> {
        /// Creates a new fast-closing file handle
        #[inline(always)]
        pub fn new(handle: H) -> Self {
            handle.fast_close()
        }
    }

    impl<H: Send + 'static> FastClose<H> {
        // Private definition for FastCloseable to use
        /// Creates a new fast-closing file handle
        #[inline]
        pub(super) fn _new(handle: H) -> FastClose<H> {
            FastClose(ManuallyDrop::new(handle))
        }

        /// Gets the interal [`OwnedHandle`]
        ///
        /// # Safety
        ///
        /// `self.0` must never be accessed again.
        /// This method should only be called on drop
        #[inline]
        unsafe fn get_handle(&mut self) -> H {
            // SAFETY: relies on self.0 never being accessed again
            unsafe { ManuallyDrop::take(&mut self.0) }
        }
    }

    impl<H: Send + 'static> Drop for FastClose<H> {
        /// Submits the file handle to a thread pool to handle its closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-threadpool")]
        fn drop(&mut self) {
            let closer_pool =
                CLOSER_POOL.get_or_init(|| ThreadPoolBuilder::new().build());
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            closer_pool.execute(move || drop(handle));
        }

        /// Submits the file handle to `rayon`'s thread pool to handle its
        /// closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-rayon")]
        fn drop(&mut self) {
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            rayon::spawn(move || drop(handle));
        }

        /// Submits the file handle as a `async-std` task to handle its
        /// closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-async-std")]
        fn drop(&mut self) {
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            async_std::task::spawn(async move { drop(handle) });
        }

        /// Submits the file handle as a `smol` task to handle its
        /// closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-smol")]
        fn drop(&mut self) {
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            smol::spawn(async move { drop(handle) }).detach();
        }
    }

    impl<H: Send + 'static> fmt::Debug for FastClose<H>
    where
        H: fmt::Debug,
    {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_tuple("FastClose").field(&self.0.deref()).finish()
        }
    }

    // Windows-only blanket impls
    impl<H: Send + 'static> AsHandle for FastClose<H>
    where
        H: AsHandle,
    {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            self.0.as_handle()
        }
    }

    impl<H: Send + 'static> FileExt for FastClose<H>
    where
        H: FileExt,
    {
        fn seek_read(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
            self.0.seek_read(buf, offset)
        }

        fn seek_write(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
            self.0.seek_write(buf, offset)
        }
    }
}

/// The non-Windows stub implementation of [`FastClose`]
#[cfg(not(windows))]
mod stub {
    use crate::FastCloseable;

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    #[derive(Debug)]
    pub struct FastClose<H: Send + 'static>(pub(super) H);

    // Public interface
    impl<H: FastCloseable> FastClose<H> {
        /// Creates a new fast-closing file handle
        #[inline(always)]
        pub fn new(handle: H) -> Self {
            handle.fast_close()
        }
    }

    impl<H: Send + 'static> FastClose<H> {
        // Private definition for FastCloseable to use
        /// Creates a new fast-closing file handle
        #[inline]
        pub(super) fn _new(handle: H) -> FastClose<H> {
            FastClose(handle)
        }
    }

    impl<H: Send + 'static> Drop for FastClose<H> {
        /// Submits the file handle to your chosen backend to handle its closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        fn drop(&mut self) {}
    }
}

impl<H: Send + 'static> Deref for FastClose<H> {
    type Target = H;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<H: Send + 'static> DerefMut for FastClose<H> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<H> From<H> for FastClose<H>
where
    H: FastCloseable,
{
    fn from(handle: H) -> Self {
        handle.fast_close()
    }
}

impl<H> io::Read for FastClose<H>
where
    H: io::Read + Send + 'static,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    fn read_vectored(
        &mut self,
        bufs: &mut [IoSliceMut<'_>],
    ) -> io::Result<usize> {
        self.0.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.0.read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.0.read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.0.read_exact(buf)
    }

    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }
}

impl<H> io::Write for FastClose<H>
where
    H: io::Write + Send + 'static,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf)
    }

    fn write_fmt(&mut self, fmt: Arguments<'_>) -> io::Result<()> {
        self.0.write_fmt(fmt)
    }

    fn by_ref(&mut self) -> &mut Self
    where
        Self: Sized,
    {
        self
    }
}

impl<H> io::Seek for FastClose<H>
where
    H: io::Seek + Send + 'static,
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }

    fn rewind(&mut self) -> io::Result<()> {
        self.0.rewind()
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        self.0.stream_position()
    }
}

// TODO: add Async traits in here for those backends

/// Indicates compatibility with [`FastClose`], providing a convenience method
/// for wrapping a type
///
/// # Implementing `FastCloseable`
///
/// `FastCloseable` can be implemented on any type that will trigger a call to
/// Windows' [`CloseHandle`](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-closehandle) on drop
///
/// Phrased another way, you can implement `FastCloseable` on any file wrapper
/// type that is:
/// - Owned, not borrowed (`'static`)
/// - Not reference counted (use [`FastClose`] **inside** of an `Arc`, not
///   outside)
/// - `Send`
/// - `!Clone`
///
/// You should use the default implementation for `fast_close()`, as it is the
/// only public API for constructing a `FastClose` that doesn't rely on the
/// `FastCloseable` trait (`FastClose::new` just calls `.fast_close()` on the
/// parameter)
pub trait FastCloseable: Send {
    /// Wraps `self` in [`FastClose`]
    #[inline(always)]
    fn fast_close(self) -> FastClose<Self>
    where
        Self: Sized,
    {
        // Use internal constructor, because the public one calls .fast_close()
        FastClose::_new(self)
    }
}

// Add compatible struct implementations
impl FastCloseable for File {}

#[cfg(feature = "backend-async-std")]
impl FastCloseable for async_std::fs::File {}

#[cfg(feature = "backend-smol")]
impl FastCloseable for smol::fs::File {}

#[cfg(test)]
mod tests {
    use std::{fs::File, mem::size_of};

    use crate::FastClose;

    #[test]
    fn is_zst() {
        assert_eq!(
            size_of::<FastClose<File>>(),
            size_of::<File>(),
            "FastClose is not a ZST"
        );
    }

    #[test]
    fn debug_repr_hides_manually_drop() {
        let file = FastClose::new(File::open("Cargo.toml").unwrap());

        let debug = format!("{file:?}");
        println!("Debug: {debug}");
        assert!(
            !debug.contains("ManuallyDrop"),
            "Debug should hide implementation details"
        );
        assert!(
            debug.contains("File"),
            "Debug (pretty) should show inner type"
        );

        let debug_pretty = format!("{file:#?}");
        println!("Pretty debug: {debug_pretty}");
        assert!(
            !debug_pretty.contains("ManuallyDrop"),
            "Debug (pretty) should hide implementation details"
        );
        assert!(
            debug_pretty.contains("File"),
            "Debug should show inner type"
        );
    }

    // TODO: add trait implementation tests
}
