# Changelog

All notable changes to this project will be documented in this file

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

## v0.3.2 - 2023/12/02

* Add `FastClose::into_inner`
* Add `repr(transparent)` explicitly to `FastClose` (both stubbed & not)

## v0.3.1 - 2023/12/02

* Remove MSRV - this is almost entirely dependent on the backend you're using

## v0.3.0 - 2023/11/11

A rewrite of `FastClose`'s compatibility - no longer relying on `Into<OwnedHandle>`, as adoption of this trait implementation is lacking across the crates.io ecosystem.
Being able to use `FastClose` on a non-std `File` equivalent will now always require explicit support either in `close_already`, or the crate providing the `File` replacement (this is due to the [orphan rule](https://doc.rust-lang.org/reference/items/implementations.html#orphan-rules) and needing to `impl FastCloseable`).
In practice, this was the case already, as none of the backends that provided their own file types had a released version with `Into<OwnedHandle>` support (at time of writing), with some not interested in adding it (e.g. [`tokio`](https://lib.rs/crates/tokio), due to their [MSRV requirements](https://github.com/tokio-rs/mio/pull/1606#issuecomment-1212491131))

* Add multiple backend support, along with six backends to choose from
  * Implement `Async{Read,Write,Seek}` for async backends
  * Support `File` equivalents provided by backends
* Add Justfile for ease for linting/testing for contributors

## v0.2.1 - 2023/11/05

* Improve `Debug` representation on Windows
* Fix GitHub/Codeberg README links
* Have docs.rs show Windows documentation (given that's most relevant)

## v0.2.0 - 2023/11/05

* Add support for other operating systems
* docs.rs now has the documentation

## v0.1.0 - 2023/11/04

* Initial Windows-only release
