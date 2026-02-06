# dumpspace-api

![Codecov](https://img.shields.io/codecov/c/github/B1Fr0st/dumpspace-api)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/NoctisMenu/dumpspace-api/audit.yml?label=audit)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/NoctisMenu/dumpspace-api/tests.yml?label=tests)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/NoctisMenu/dumpspace-api/publish.yml?label=publish)
![Crates.io Total Downloads](https://img.shields.io/crates/d/dumpspace-api)
![Crates.io Version](https://img.shields.io/crates/v/dumpspace-api)

Crate that allows you to have static, always up to date offsets for any game supported on Dumpspace.

## Usage

```rust
use dumpspace_api::*;

setup!("6b77eceb"); // must be called at the top of the crate root with the dumpspace game hash

fn main() {
    let off = offset!("UWorld", "OwningGameInstance"); // literal 0x228
    let size = class_size!("AActor");
    let gworld = global_offset!("OFFSET_GWORLD");
    let name = enum_name!("EFortRarity", 1); // "EFortRarity__Uncommon"
}

```

## Features

* Automatic caching and cache invalidation on offset update
* Zero cost at runtime; all offsets are generated at compile time
* Fully threadsafe and no memory requirement

[Docs](https://docs.rs/dumpspace-api/)

Project based on Spuckwaffel's original [C++ API](https://github.com/Spuckwaffel/Dumpspace-API).
