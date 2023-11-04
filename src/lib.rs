#![deny(clippy::undocumented_unsafe_blocks)]
#![warn(missing_docs)]

use std::{
    io,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    os::windows::io::OwnedHandle,
    sync::OnceLock,
};

use threadpool::ThreadPool;

static CLOSER_POOL: OnceLock<ThreadPool> = OnceLock::new();

#[derive(Debug)]
pub struct FastClose<H: Into<OwnedHandle> + ?Sized>(ManuallyDrop<H>);

impl<H> FastClose<H>
where
    H: Into<OwnedHandle>,
{
    #[inline(always)]
    pub fn new(handle: H) -> Self {
        FastClose(ManuallyDrop::new(handle))
    }
}

impl<H> Drop for FastClose<H>
where
    H: Into<OwnedHandle>,
{
    fn drop(&mut self) {
        let closer_pool =
            CLOSER_POOL.get_or_init(|| ThreadPool::new(num_cpus::get()));
        // SAFETY: we're in Drop, so self.0 won't be accessed again
        let handle = unsafe { ManuallyDrop::take(&mut self.0) }.into();
        closer_pool.execute(move || drop(handle));
    }
}

impl<H> From<H> for FastClose<H>
where
    H: Into<OwnedHandle>,
{
    fn from(handle: H) -> Self {
        Self::new(handle)
    }
}

impl<H> Deref for FastClose<H>
where
    H: Into<OwnedHandle>,
{
    type Target = H;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<H> DerefMut for FastClose<H>
where
    H: Into<OwnedHandle>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait FastCloseable: Sized
where
    OwnedHandle: From<Self>,
{
    fn fast_close(self) -> FastClose<Self>;
}

impl<H> FastCloseable for H
where
    OwnedHandle: From<Self>,
{
    #[inline(always)]
    fn fast_close(self) -> FastClose<Self> {
        FastClose::new(self)
    }
}

/// Fast-closing replacements to the standard library filesystem manipulation
/// operations
///
/// This module provides replaces the standard library functions with
/// `close_already`-using versions. The functions have identical signatures to
/// make drop-in replacing possible, and near identical code (exceptions noted
/// in documentation for specific methods)
pub mod fs {
    use std::{
        fs::{File, OpenOptions},
        io,
        io::{Read, Write},
        path::Path,
    };

    use super::FastCloseable;

    /// Copies the contents of one file to another.
    /// This function will also copy the permission bits of the original file to
    /// the destination file
    ///
    /// This function will **overwrite** the contents of `to`
    ///
    /// Note that if `from` and `to` both point to the same file, then the file
    /// will likely get truncated by this operation
    ///
    /// On success, the total number of bytes copied is returned and it is equal
    /// to the length of the `to` file as reported by `metadata`
    ///
    /// # `close_already` differences
    ///
    /// This function is entirely re-implemented to open files and then delegate
    /// to [`std::io::copy()`].
    /// After the copy is completed, the permission bits are set
    pub fn copy(
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> io::Result<u64> {
        fn inner(from_path: &Path, to_path: &Path) -> io::Result<u64> {
            let mut from = File::open(from_path)?.fast_close();
            let mut to = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(to_path)?
                .fast_close();
            let copied = io::copy(&mut from, &mut to)?;
            std::fs::set_permissions(to_path, from.metadata()?.permissions())?;
            Ok(copied)
        }
        inner(from.as_ref(), to.as_ref())
    }

    /// Read the entire contents of a file into a bytes vector
    ///
    /// This is a convenience function for using [`File::open`] and
    /// [`read_to_end`](Read::read_to_end) with fewer imports and without an
    /// intermediate variable
    ///
    /// # `close_already` differences
    ///
    /// The standard library uses a private function which gives a size hint to
    /// `read_to_end`, presumably making it slightly more efficient than not
    /// being able to provide a size hint. Otherwise, the implementation is
    /// identical
    pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        fn inner(path: &Path) -> io::Result<Vec<u8>> {
            let mut file = File::open(path)?.fast_close();
            let size = file.metadata().map(|m| m.len() as usize).ok();
            let mut bytes = Vec::with_capacity(size.unwrap_or(0));
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        }
        inner(path.as_ref())
    }

    /// Read the entire contents of a file into a string.
    ///
    /// This is a convenience function for using [`File::open`] and
    /// [`read_to_string`](Read::read_to_string) with fewer imports and
    /// without an intermediate variable
    ///
    /// # `close_already` differences
    ///
    /// The standard library uses a private function which gives a size hint to
    /// `read_to_string`, presumably making it slightly more efficient than not
    /// being able to provide a size hint. Otherwise, the implementation is
    /// identical
    pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
        fn inner(path: &Path) -> io::Result<String> {
            let mut file = File::open(path)?;
            let size = file.metadata().map(|m| m.len() as usize).ok();
            let mut string = String::with_capacity(size.unwrap_or(0));
            file.read_to_string(&mut string)?;
            Ok(string)
        }
        inner(path.as_ref())
    }

    /// Write a slice as the entire contents of a file
    ///
    /// This function will create a file if it does not exist,
    /// and will entirely replace its contents if it does
    ///
    /// Depending on the platform, this function may fail if the
    /// full directory path does not exist
    ///
    /// This is a convenience function for using [`File::create`] and
    /// [`write_all`](Write::write_all) with fewer imports
    ///
    /// # `close_already` differences
    ///
    /// None
    pub fn write(
        path: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<()> {
        fn inner(path: &Path, contents: &[u8]) -> io::Result<()> {
            File::create(path)?.fast_close().write_all(contents)
        }
        inner(path.as_ref(), contents.as_ref())
    }
}

// Blanket impls go here
// TODO: Read, Seek, Write, Into<OwnedHandle>, IntoRawHandle
impl<H> io::Read for FastClose<H>
where
    H: io::Read + Into<OwnedHandle>,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    // TODO: delegate specialised methods too
}

impl<H> io::Write for FastClose<H>
where
    H: io::Write + Into<OwnedHandle>,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
