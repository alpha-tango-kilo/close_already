#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

use std::{
    fmt::Arguments,
    io,
    io::{IoSlice, IoSliceMut, SeekFrom},
    ops::{Deref, DerefMut},
};

// cfg specification for having either std::os::windows(::io::OwnedHandle) or
// std::os::fd(::OwnedFd)
#[cfg(not(any(windows, unix, target_os = "wasi")))]
compile_error!(
    "close_already doesn't support this target. Open an issue and let's \
     discuss!"
);

mutually_exclusive_features::exactly_one_of! {
    "backend-async-std",
    "backend-rayon",
    "backend-smol",
    "backend-threadpool",
}

#[cfg(not(windows))]
pub use stub::*;
#[cfg(windows)]
pub use windows::*;

pub mod fs;

#[cfg(windows)]
mod windows {
    #[cfg(feature = "backend-threadpool")]
    use std::sync::OnceLock;
    use std::{
        fmt, io,
        mem::ManuallyDrop,
        ops::Deref,
        os::windows::{io::OwnedHandle, prelude::*},
    };

    #[cfg(feature = "backend-threadpool")]
    use threadpool::{Builder as ThreadPoolBuilder, ThreadPool};

    #[cfg(feature = "backend-threadpool")]
    static CLOSER_POOL: OnceLock<ThreadPool> = OnceLock::new();

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    pub struct FastClose<H: Into<OwnedHandle> + ?Sized>(
        pub(super) ManuallyDrop<H>,
    );

    impl<H> FastClose<H>
    where
        H: Into<OwnedHandle>,
    {
        /// Creates a new fast-closing file handle
        ///
        /// You may find it more convenient to use
        /// [FastCloseable::fast_close()](crate::FastCloseable::fast_close)
        #[inline(always)]
        pub fn new(handle: H) -> Self {
            FastClose(ManuallyDrop::new(handle))
        }
    }

    impl<H> Drop for FastClose<H>
    where
        H: Into<OwnedHandle>,
    {
        /// Submits the file handle to a thread pool to handle its closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-threadpool")]
        fn drop(&mut self) {
            let closer_pool =
                CLOSER_POOL.get_or_init(|| ThreadPoolBuilder::new().build());
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { ManuallyDrop::take(&mut self.0) }.into();
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
            let handle = unsafe { ManuallyDrop::take(&mut self.0) }.into();
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
            let handle = unsafe { ManuallyDrop::take(&mut self.0) }.into();
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
            let handle = unsafe { ManuallyDrop::take(&mut self.0) }.into();
            smol::spawn(async move { drop(handle) }).detach();
        }
    }

    impl<H> fmt::Debug for FastClose<H>
    where
        H: fmt::Debug + Into<OwnedHandle>,
    {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_tuple("FastClose").field(&self.0.deref()).finish()
        }
    }

    // Windows-only blanket impls
    impl<H> AsHandle for FastClose<H>
    where
        H: AsHandle + Into<OwnedHandle>,
    {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            self.0.as_handle()
        }
    }

    impl<H> FileExt for FastClose<H>
    where
        H: FileExt + Into<OwnedHandle>,
    {
        fn seek_read(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
            self.0.seek_read(buf, offset)
        }

        fn seek_write(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
            self.0.seek_write(buf, offset)
        }
    }
}

#[cfg(not(windows))]
mod stub {
    use std::os::fd::OwnedFd;

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    #[derive(Debug)]
    pub struct FastClose<H: Into<OwnedFd> + ?Sized>(pub(super) H);

    impl<H> FastClose<H>
    where
        H: Into<OwnedFd> + ?Sized,
    {
        /// Creates a new fast-closing file handle
        ///
        /// You may find it more convenient to use
        /// [FastCloseable::fast_close()](crate::FastCloseable::fast_close)
        pub fn new(handle: H) -> Self {
            FastClose(handle)
        }
    }

    impl<H> Drop for FastClose<H>
    where
        H: Into<OwnedFd> + ?Sized,
    {
        /// Submits the file handle to a thread pool to handle its closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        fn drop(&mut self) {}
    }
}

// Blanket impls that work for stub and non-stub go here
macro_rules! blanket_impls {
    ($handle_type:path) => {
        impl<H> Deref for FastClose<H>
        where
            H: Into<$handle_type>,
        {
            type Target = H;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<H> DerefMut for FastClose<H>
        where
            H: Into<$handle_type>,
        {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl<H> From<H> for FastClose<H>
        where
            H: Into<$handle_type>,
        {
            fn from(handle: H) -> Self {
                Self::new(handle)
            }
        }

        impl<H> io::Read for FastClose<H>
        where
            H: io::Read + Into<$handle_type>,
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

            fn read_to_string(
                &mut self,
                buf: &mut String,
            ) -> io::Result<usize> {
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
            H: io::Write + Into<$handle_type>,
        {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.write(buf)
            }

            fn write_vectored(
                &mut self,
                bufs: &[IoSlice<'_>],
            ) -> io::Result<usize> {
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
            H: io::Seek + Into<$handle_type>,
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
    };
}

#[cfg(windows)]
blanket_impls!(std::os::windows::io::OwnedHandle);

// Use OwnedFd as the non-Windows alternative that *should* work in most cases
// (I would expect it to be very rare for a type to impl Into<OwnedFd> on *nix
// and then not impl Into<OwnedHandle> on Windows.) If that's an issue, then
// that's probably time to offload the complexity of conditional compile
// wizardry onto the crate user, as I can't really make a trait bound for
// "implements this trait on this other OS" (as far as I know!)
#[cfg(not(windows))]
blanket_impls!(std::os::fd::OwnedFd);

// Convenience helpers

macro_rules! fast_closeable {
    ($handle_type:path) => {
        /// Provides a convenience method to chain with that wraps a file handle
        /// with [`FastClose`]
        pub trait FastCloseable: Sized
        where
            $handle_type: From<Self>,
        {
            /// Wraps `self` in [`FastClose`]
            fn fast_close(self) -> FastClose<Self>;
        }

        impl<H> FastCloseable for H
        where
            $handle_type: From<Self>,
        {
            #[inline(always)]
            fn fast_close(self) -> FastClose<Self> {
                FastClose::new(self)
            }
        }
    };
}

#[cfg(windows)]
fast_closeable!(std::os::windows::io::OwnedHandle);

#[cfg(not(windows))]
fast_closeable!(std::os::fd::OwnedFd);

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
}
