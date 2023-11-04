# `close_already` - speeding up programs handling lots of files on Windows

**Closing files on Windows is slow, taking 1-10 milliseconds compared to microseconds (on MacOS, Linux, and friends).**
The reasoning is explained in [this blog post](https://gregoryszorc.com/blog/2021/04/06/surprisingly-slow/) by Gregory Szorc, which also suggests using thread pools to handle the closing of file handles on Windows.
This is exactly what this library implements, while being as unintruisive to the developer as possible.
While not using this crate specifically, there are case studies in both [rustup](https://github.com/rust-lang/rustup/pull/1850) and [Mercurial](https://repo.mercurial-scm.org/hg/rev/2fdbf22a1b63f7b4c94393dbf45ad417da257fe0) where this has massively improved performance.

## Should I use it?

If you're reading/writing relatively small files in the order of magnitude of hundreds or greater, you would most likely benefit from `close_already`.
It's designed to be easy to switch to and use, so try it out and benchmark it!
Note that if your code is already trying to use multiple threads/cores to handle files (e.g. with `rayon`, your performance gains will be far more modest)

`close_already` is **not** async-friendly currently, using OS threads instead of green threads.
I may add an async backend at some point, or otherwise I would welcome a PR supporting your preferred runtime

## How do I use it?

To add it to your project:

```shell
cargo add --target 'cfg(windows)' close_already
```

Provided your type supports `Into<std::os::windows::io::OwnedHandle>` (which `std::fs::File` does), then you can eithe construct a [`FastClose`] with [`FastClose::new`], or take advantage of the [`FastCloseable`] trait and call `.fast_close()` to wrap your type.
That's it.

Or if you're more of a `std::fs::read` and `std::fs::write` user, then all the functions that can take advantage of `close_already` have been re-implemented in the `fs` module

## How does `close_already` work?

As explained, the basic principle is to provide a threadpool which handles file closures

This implementation uses a zero-sized wrapper type [`FastClose`] (no memory overhead, woo!), which has a custom [`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) implementation, which will send the file handle to a thread pool when it's no longer needed, to allow multiple threads to parallelise the waiting time for file closures.
The thread pool is lazily initialised when the first [`FastClose`] is dropped (using the newly stabilised [`OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html))

The [`FastClose`] struct implements [`Deref`](https://doc.rust-lang.org/std/ops/trait.Deref.html) and [`DerefMut`](https://doc.rust-lang.org/std/ops/trait.DerefMut.html), meaning you can completely ignore its existence for all intents and purposes, and then let the magic happen as it goes out of scope

The best part is how concise the solution is to implement, with the basic core logic taking under 30 lines; with most of the bulk coming from delegating trait implementations and providing standard library convenience function equivalents

### Does it work?

In short, yes, almost concerningly well.
Proper benchmarks incoming, but using a patched version of [`norad`](https://github.com/linebender/norad) - a library for manipulating Unified Font Objects (a font source format notorious for having hundreds or thousands of small files) - I observed a 67% increase in write performance while `norad` was running single-threaded, or when enabling its `rayon` feature, I still observed a ~10% speed-up, despite a sub-optimal implementation (conflicting threadpools)

## Contributing

### I want to add support for _____ backend!

Go for it!
Put it behind a feature gate, ensure that you can't have multiple backends enabled, and then change the type within `CLOSER_POOL` and add an implementation of `Drop` for `FastClose` that submits the `OwnedHandle` to your pool.
Everything else just works!

### I want to add support for _____ trait that I need!

Go for it!
Make sure the generic bounds include `H: Into<OwnedHandle>`, and it should work out just fine.
If the trait you're adding support for is not part of the standard library (or is on nightly), please put it behind a feature gate (default off)

## License

MIT or Apache 2, at your option (the same as Rust itself)
