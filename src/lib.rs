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

pub trait FastCloseWrap: Sized
where
    OwnedHandle: From<Self>,
{
    fn fast_close(self) -> FastClose<Self>;
}

impl<H> FastCloseWrap for H
where
    OwnedHandle: From<Self>,
{
    #[inline(always)]
    fn fast_close(self) -> FastClose<Self> {
        FastClose::new(self)
    }
}

pub mod fs {
    use std::{fs::File, io, io::Write, path::Path};

    use crate::FastCloseWrap;

    pub fn write(
        path: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<()> {
        fn inner(path: &Path, contents: &[u8]) -> io::Result<()> {
            File::create(path)?.fast_close().write_all(contents)
        }
        inner(path.as_ref(), contents.as_ref())
    }

    pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        todo!()
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
