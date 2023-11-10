# `close_already` - speeding up programs handling lots of files on Windows

[![Codeberg CI](https://ci.codeberg.org/api/badges/alpha-tango-kilo/close_already/status.svg)](https://ci.codeberg.org/alpha-tango-kilo/close_already)
[![Crates.io](https://img.shields.io/crates/v/close_already.svg)](https://crates.io/crates/close_already)
[![Dependencies](https://deps.rs/repo/codeberg/alpha-tango-kilo/close_already/status.svg)](https://deps.rs/repo/codeberg/alpha-tango-kilo/close_already)

**Closing files on Windows is slow, taking 1-10 milliseconds compared to microseconds on MacOS, Linux, and friends.**
The "why?" is explained in [this blog post](https://gregoryszorc.com/blog/2021/04/06/surprisingly-slow/) by Gregory Szorc, which also suggests using thread pools to handle the closing of file handles on Windows.
This is exactly what this crate implements, while being as unintruisive to the developer as possible.
While not using this crate specifically, there are case studies in both [rustup](https://github.com/rust-lang/rustup/pull/1850) and [Mercurial](https://repo.mercurial-scm.org/hg/rev/2fdbf22a1b63f7b4c94393dbf45ad417da257fe0) where this technique has massively improved performance

## Should I use it?

If you're reading/writing relatively small files in the order of magnitude of hundreds or greater, you would most likely benefit from `close_already`.
It's designed to be easy to switch to and use, so try it out and benchmark it!
Note that if your code is already trying to use multiple threads/cores to handle files (e.g. with `rayon`), your performance gains will be far more modest

### Compatibility
<!-- If you change this heading name, change the heading link in the install section -->

Each listed backend comes with a corresponding feature `backend-<name>`.
To use a non-default backend, set `default-features = false` and enable the corresponding `backend-<name>` feature

Supported backends:
* [`threadpool`](https://lib.rs/crates/threadpool) - default, creates and uses its own OS-thread threadpool
* [`rayon`](https://lib.rs/crates/rayon) - uses rayon's global threadpool
* [`async-std`](https://lib.rs/crates/async-std) - uses `async-std`'s global executor. `async_std`'s `File` supports `close_already`
* [`smol`](https://lib.rs/crates/smol) - uses `smol`'s global executor. `smol`'s `File` supports `close_already`

#### Why not `tokio`?

`tokio`'s [`File`](https://docs.rs/tokio/latest/tokio/fs/struct.File.html) does not support `IntoRawHandle` on Windows.
It would be possible to add a backend to `close_already` to spawn `tokio` tasks, but you'd have to use `std::fs::File` instead of `tokio`'s, meaning no access to `Async{Read,Write}`

## How do I use it?

To add it to your project using the default [`threadpool`](https://lib.rs/crates/threadpool) backend:

```shell
cargo add close_already
```

Or with a different backend (see [compatibility](#compatibility) for available backends):

```shell
cargo add close_already -F backend-<name> --no-default-features
```

Provided your type supports `Into<std::os::windows::io::OwnedHandle>` (which `std::fs::File` does), then you can either construct a [`FastClose`](https://docs.rs/close_already/latest/close_already/struct.FastClose.html) with [`FastClose::new`](https://docs.rs/close_already/latest/close_already/struct.FastClose.html#method.new), or take advantage of the [`FastCloseable`](https://docs.rs/close_already/latest/close_already/trait.FastCloseable.html) trait and call `.fast_close()` to wrap your type.
That's it.

Or if you're more of a `std::fs::read` and `std::fs::write` user, then all the functions that can take advantage of `close_already` have been re-implemented in the `fs` module

### What if I'm not always targeting/developing on Windows?

That will usually be fine. 
`FastClose` simply won't create/use a threadpool and send file closures to it, but all the same structs/methods/traits will be available so you don't need conditional compilation `#[cfg]`s everywhere

Where on Windows `FastClose` supports types that are `Into<OwnedHandle>`, on not-Windows instead the trait bound is `Into<OwnedFd>`.
This is the closest approximation I can get, but if there's a type that supports one trait but not the other, then `FastClose` won't be able to help you and you'll need `#[cfg]` annotations etc. to avoid using `FastClose` when not on Windows

Also, if you're on a very rare OS that doesn't have either `OwnedHandle` or `OwnedFd`, then `close_already` simply won't compile and you'll need to make the dependency OS-dependent (or contribute support for that OS!)
You can do this by putting the dependency under `[target.'cfg(windows)'.dependencies]` in Cargo.toml, and then using conditional flags where necessary in your code

## How does `close_already` work?

As explained, the basic principle is to provide a threadpool which handles file closures

This implementation uses a zero-sized wrapper type [`FastClose`](https://docs.rs/close_already/latest/close_already/struct.FastClose.html) (no memory overhead, woo!), which has a custom [`Drop`](https://doc.rust-lang.org/std/ops/trait.Drop.html) implementation, which will send the file handle to a thread pool when it's no longer needed, to allow multiple threads to parallelise the waiting time for file closures.
The thread pool is lazily initialised when the first [`FastClose`](https://docs.rs/close_already/latest/close_already/struct.FastClose.html) is dropped (using the newly stabilised [`OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html))*

The [`FastClose`](https://docs.rs/close_already/latest/close_already/struct.FastClose.html) struct implements [`Deref`](https://doc.rust-lang.org/std/ops/trait.Deref.html) and [`DerefMut`](https://doc.rust-lang.org/std/ops/trait.DerefMut.html), meaning you can completely ignore its existence for all intents and purposes, and then let the magic happen as it goes out of scope

The best part is how concise the solution is to implement, with the basic core logic taking under 30 lines; with most of the bulk coming from delegating trait implementations and providing standard library convenience function equivalents

(* on non-`threadpool` backends, the global thread pool / executor is used)

## Does it work?

In short, yes, almost concerningly well.
Proper benchmarks incoming, but using a patched version of [`norad`](https://github.com/linebender/norad) - a library for manipulating Unified Font Objects (a font source format notorious for having hundreds or thousands of small files) - I observed a 67% increase in write performance while `norad` was running single-threaded, or when enabling its `rayon` feature, I still observed a ~10% speed-up, despite a sub-optimal implementation (conflicting threadpools)

## Contributing

There's a [Justfile](https://github.com/casey/just#readme) for ease of running checks & tests across multiple backends.
It requires [`cargo-hack`](https://lib.rs/crates/cargo-hack) to be installed, and the `x86_64-pc-windows-msvc` target for your toolchain.
Run `just` to see available recipes

Please ensure your code is formatted with **nightly** `rustfmt` and there are no Clippy lints for any backend when submitting your PR

### I want to add support for _____ backend!

Go for it!
Put it behind a feature gate, add the feature name to the `mutually_exclusive_features::exactly_one_of!` block at the top of `lib.rs`, and then add a new definition of `Drop::drop` for `windows::FastClose` that's enabled by your feature flag.
If you're lazily initialising your own thread pool / executor, you'll naturally need a `static OnceLock` as well, the same as how `backend-threadpool` works.
That's it!

### I want to add support for _____ trait that I need!

Go for it!
Make sure the generic bounds include `H: Into<OwnedHandle>`, and it should work out just fine.
If the trait you're adding support for is not part of the standard library (or is on nightly), please put it behind a feature gate (default off)

## License

MIT or Apache 2, at your option (the same as Rust itself)
