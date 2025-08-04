# dumpspace-api

![Codecov](https://img.shields.io/codecov/c/github/B1Fr0st/dumpspace-api)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/B1Fr0st/dumpspace-api/audit.yml?label=audit)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/B1Fr0st/dumpspace-api/tests.yml?label=tests)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/B1Fr0st/dumpspace-api/publish.yml?label=publish)
![Crates.io Total Downloads](https://img.shields.io/crates/d/dumpspace-api)
![Crates.io Version](https://img.shields.io/crates/v/dumpspace-api)

The dumpspace API allows you to get your games' info directly from the Dumpspace website to use it in your Rust project, using `reqwest::blocking` for non-async compatibility.

Project based on Spuckwaffel's original C++ API, I just rewrote it in Rust and added unit tests. Refer to the [C++ API](https://github.com/Spuckwaffel/Dumpspace-API) for any actual questions.

Features added on top of the C++ dumpspace API:

* Offset caching for reduced startup times + bandwidth reduction
* Automatic cache invalidation on game update

[Docs](https://docs.rs/dumpspace-api/)
