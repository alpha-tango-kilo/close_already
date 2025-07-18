#![doc = include_str!("../README.md")]

use std::{
    cmp::Ordering,
    fmt::Arguments,
    io,
    io::{IoSlice, IoSliceMut, SeekFrom},
    ops::{Deref, DerefMut},
};

mutually_exclusive_features::exactly_one_of! {
    "backend-async-std",
    "backend-blocking",
    "backend-rayon",
    "backend-smol",
    "backend-threadpool",
    "backend-tokio",
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

    /// A lazily initialised [`ThreadPool`] to send handle closures to
    #[cfg(feature = "backend-threadpool")]
    static CLOSER_POOL: OnceLock<ThreadPool> = OnceLock::new();

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    #[repr(transparent)]
    #[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
    pub struct FastClose<H: Send + 'static>(pub(super) ManuallyDrop<H>);

    impl<H: Send + 'static> FastClose<H> {
        /// Gets back the inner file type
        ///
        /// This means that `close_already` will no longer send the handle to a
        /// backend on drop
        pub fn into_inner(self) -> H {
            // Prevent destructor being called first, in case we get interrupted
            // somehow before the end of the method
            let mut wrapped = ManuallyDrop::new(self);
            // SAFETY: we are never going to access self.0 again because this
            // method takes ownership of self and we've already prevented its
            // destructor from being called
            unsafe { ManuallyDrop::take(&mut wrapped.0) }
        }

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

        /// Submits the file handle as a `blocking` task to handle its
        /// closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-blocking")]
        fn drop(&mut self) {
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            blocking::unblock(move || drop(handle)).detach();
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

        /// Submits the file handle as a `tokio` task to handle its
        /// closure
        ///
        /// Note: on non-Windows targets, nothing is done, the handle is just
        /// dropped normally
        #[cfg(feature = "backend-tokio")]
        fn drop(&mut self) {
            // SAFETY: we're in Drop, so self.0 won't be accessed again
            let handle = unsafe { self.get_handle() };
            tokio::task::spawn(async move { drop(handle) });
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
    use std::{mem::ManuallyDrop, ptr};

    /// A zero-sized wrapper that moves a file handle to a thread pool on drop
    #[repr(transparent)]
    #[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
    pub struct FastClose<H: Send + 'static>(pub(super) H);

    impl<H: Send + 'static> FastClose<H> {
        // https://discord.com/channels/442252698964721669/443150878111694848/1180556717243764829
        /// Gets back the inner file type
        ///
        /// This means that `close_already` will no longer send the handle to a
        /// backend on drop
        // Note: ideally we'd transmute here, but the compiler as of 1.74
        // currently won't transmute "dependently-sized types", which
        // FastClose is (being generic over H)
        pub fn into_inner(self) -> H {
            // Prevent destructor being called first, in case we get interrupted
            // somehow before the end of the method
            let wrapped = ManuallyDrop::new(self);
            let h_ptr: *const H = &wrapped.0;
            // SAFETY: we know h_ptr points to H still because the it was
            // wrapped in ManuallyDrop, preventing its destructor being run
            unsafe { ptr::read(h_ptr) }
        }

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

// Public interface goes here
impl<H: FastCloseable> FastClose<H> {
    /// Creates a new fast-closing file handle
    #[inline(always)]
    pub fn new(handle: H) -> Self {
        handle.fast_close()
    }
}

// Method impls for stub or non-stub
impl<H: Send + 'static> FastClose<H> {
    /// Pin projects from `self` to the inner file handle
    #[cfg(any(
        feature = "backend-async-std",
        feature = "backend-smol",
        feature = "backend-tokio",
    ))]
    #[inline]
    fn pin_project_to_inner(
        self: std::pin::Pin<&mut Self>,
    ) -> std::pin::Pin<&mut H> {
        // SAFETY: `self.0` is pinned when `self` is pinned
        unsafe { self.map_unchecked_mut(|fc| fc.deref_mut()) }
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

impl<H> PartialEq<H> for FastClose<H>
where
    H: PartialEq + Send + 'static,
{
    fn eq(&self, other: &H) -> bool {
        self.deref() == other
    }
}

impl<H> PartialOrd<H> for FastClose<H>
where
    H: PartialOrd + Send + 'static,
{
    fn partial_cmp(&self, other: &H) -> Option<Ordering> {
        self.deref().partial_cmp(other)
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

impl FastCloseable for std::fs::File {}

/// Trait implementations for `async-std` types
#[cfg(feature = "backend-async-std")]
mod async_std_impls {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use async_std::io::{
        Read as AsyncRead, Seek as AsyncSeek, Write as AsyncWrite,
    };

    use super::*;

    impl FastCloseable for async_std::fs::File {}

    impl<H> AsyncRead for FastClose<H>
    where
        H: AsyncRead + Send + 'static,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_read(cx, buf)
        }

        fn poll_read_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &mut [IoSliceMut<'_>],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_read_vectored(cx, bufs)
        }
    }

    impl<H> AsyncSeek for FastClose<H>
    where
        H: AsyncSeek + Send + 'static,
    {
        fn poll_seek(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            pos: SeekFrom,
        ) -> Poll<io::Result<u64>> {
            self.pin_project_to_inner().poll_seek(cx, pos)
        }
    }

    impl<H> AsyncWrite for FastClose<H>
    where
        H: AsyncWrite + Send + 'static,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_write(cx, buf)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_write_vectored(cx, bufs)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            self.pin_project_to_inner().poll_flush(cx)
        }

        fn poll_close(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            self.pin_project_to_inner().poll_close(cx)
        }
    }
}

/// Trait implementations for `smol` types
#[cfg(feature = "backend-smol")]
mod smol_impls {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use smol::io::{AsyncRead, AsyncSeek, AsyncWrite};

    use super::*;

    impl FastCloseable for smol::fs::File {}

    impl<H> AsyncRead for FastClose<H>
    where
        H: AsyncRead + Send + 'static,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_read(cx, buf)
        }

        fn poll_read_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &mut [IoSliceMut<'_>],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_read_vectored(cx, bufs)
        }
    }

    impl<H> AsyncSeek for FastClose<H>
    where
        H: AsyncSeek + Send + 'static,
    {
        fn poll_seek(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            pos: SeekFrom,
        ) -> Poll<io::Result<u64>> {
            self.pin_project_to_inner().poll_seek(cx, pos)
        }
    }

    impl<H> AsyncWrite for FastClose<H>
    where
        H: AsyncWrite + Send + 'static,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_write(cx, buf)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            self.pin_project_to_inner().poll_write_vectored(cx, bufs)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            self.pin_project_to_inner().poll_flush(cx)
        }

        fn poll_close(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<()>> {
            self.pin_project_to_inner().poll_close(cx)
        }
    }
}

/// Trait implementations for `tokio` types
#[cfg(feature = "backend-tokio")]
mod tokio_impls {
    use std::{
        io::Error,
        pin::Pin,
        task::{Context, Poll},
    };

    use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

    use super::*;

    impl FastCloseable for tokio::fs::File {}

    impl<H> AsyncRead for FastClose<H>
    where
        H: AsyncRead + Send + 'static,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.pin_project_to_inner().poll_read(cx, buf)
        }
    }

    impl<H> AsyncSeek for FastClose<H>
    where
        H: AsyncSeek + Send + 'static,
    {
        fn start_seek(
            self: Pin<&mut Self>,
            position: SeekFrom,
        ) -> io::Result<()> {
            self.pin_project_to_inner().start_seek(position)
        }

        fn poll_complete(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<u64>> {
            self.pin_project_to_inner().poll_complete(cx)
        }
    }

    impl<H> AsyncWrite for FastClose<H>
    where
        H: AsyncWrite + Send + 'static,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, Error>> {
            self.pin_project_to_inner().poll_write(cx, buf)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Error>> {
            self.pin_project_to_inner().poll_flush(cx)
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), Error>> {
            self.pin_project_to_inner().poll_shutdown(cx)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<Result<usize, Error>> {
            self.pin_project_to_inner().poll_write_vectored(cx, bufs)
        }

        fn is_write_vectored(&self) -> bool {
            self.0.is_write_vectored()
        }
    }
}

#[cfg(all(test, not(miri)))]
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
        assert!(debug.contains("File"), "Debug should show inner type");

        let debug_pretty = format!("{file:#?}");
        println!("Pretty debug: {debug_pretty}");
        assert!(
            !debug_pretty.contains("ManuallyDrop"),
            "Debug (pretty) should hide implementation details"
        );
        assert!(
            debug_pretty.contains("File"),
            "Debug (pretty) should show inner type"
        );

        #[cfg(feature = "backend-tokio")]
        {
            // Dropping `FastClose` without being in a tokio runtime will cause
            // a panic
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let _guard = runtime.enter();
            drop(file);
        }
    }

    #[cfg(any(
        feature = "backend-async-std",
        feature = "backend-smol",
        feature = "backend-tokio",
    ))]
    mod async_traits {
        // Import fudging spaghetti to keep the tests clean & without
        // duplication
        #[cfg(feature = "backend-async-std")]
        use async_std::{
            fs::File, io::prelude::*, io::SeekFrom, task as runtime,
        };
        #[cfg(feature = "backend-smol")]
        use smol::{self as runtime, fs::File, io::*};
        #[cfg(feature = "backend-tokio")]
        use tokio::{fs::File, io::*};
        #[cfg(feature = "backend-tokio")]
        use tokio_shim::RuntimeShim as runtime;

        // This piece of jank means that I can run a future on a Tokio runtime
        // as a static function. I either write this hack, or have to re-write
        // all the async_traits test in the tokio way (tm)
        #[cfg(feature = "backend-tokio")]
        mod tokio_shim {
            pub struct RuntimeShim;

            impl RuntimeShim {
                pub fn block_on<F: std::future::Future>(
                    future: F,
                ) -> F::Output {
                    tokio::runtime::Builder::new_current_thread()
                        .build()
                        .unwrap()
                        .block_on(future)
                }
            }
        }

        #[test]
        fn read() {
            runtime::block_on(async {
                let mut file = File::open("Cargo.toml").await.unwrap();
                let mut buf = [0; 5];
                let _ = file.read(&mut buf).await.expect("read should succeed");
            });
        }

        #[test]
        fn seek() {
            runtime::block_on(async {
                let mut file = File::open("Cargo.toml").await.unwrap();
                file.seek(SeekFrom::End(5))
                    .await
                    .expect("seek should succeed");
            });
        }

        #[test]
        fn write() {
            let std_file = tempfile::tempfile().unwrap();
            runtime::block_on(async move {
                let mut file = File::from(std_file);
                file.write_all(&[1, 2, 3])
                    .await
                    .expect("write should succeed");
            });
        }
    }
}

#[cfg(all(test, miri))]
mod miri_tests {
    use crate::{FastClose, FastCloseable};

    struct Foo;
    impl FastCloseable for Foo {}

    #[test]
    fn into_inner() {
        let fast_close = FastClose::new(Foo);
        let _ = fast_close.into_inner();
    }

    #[test]
    #[cfg(not(feature = "backend-tokio"))]
    fn drop() {
        let fast_close = FastClose::new(Foo);
        std::mem::drop(fast_close);
    }

    #[cfg(feature = "backend-tokio")]
    #[cfg_attr(feature = "backend-tokio", tokio::test)]
    async fn drop() {
        let fast_close = FastClose::new(Foo);
        std::mem::drop(fast_close);
    }
}
