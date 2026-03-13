## Index
- [Features](#features)
- [build dependencies](#builddebug-dependencies)
- [How to Init](#how-to-init)
- [How to Build](#how-to-build)
- [what is log_viewer](#what-is-the-logviewer-git-module)

## features
* loading screen **before** `exit_boot_services`
* HDR support (both pre/post exit_boot_services)
* **True Type Font(ttf)** support
###  Enable/disable features during build

#### Runtime checks
* [x] Essentials
* [x] Normal
* [x] Overprotective
* [x] **Boot options**: Memory check, boot, and shutdown.
* [x] **Full memory check**: Built-in check (patterns: addr, 0x00, 0xff, 0x55, 0xAA).
* [ ] **Overprotective**: Built-in but disabled by default.
* [ ] **Ligatures**: Powered by `rustybizz`. Available both before and after `exit_boot_services`.
* [x] **UART and more!**: See `Cargo.toml` for details.

## build/debug dependencies

* qemu-system-x86_64
* xorriso
* mkfs.msdos(dosfstools)
* ovmf

## how to init?

> [!NOTE]
> Primary development is done on Linux.
> While `.bat` files are provided for Windows,
> they are experimental.
> **WSL2 is highly recommended** for a stable build environment.
> [!NOTE]
> *Build System Internals*
> Most cargo make tasks are simple wrappers around the scripts in `scripts/`.
> You can achieve the same results by,
> manually executing the corresponding script file(just avoid those starting with `internal_`).

1. install `cargo-make`
> run 
> ```
> cargo install cargo-make
> ```

2. init project
> ```
> cargo make init_project
> # or
> ./init.(sh/bat)
> ```

> [!TIP]
> `scripts/internal_init_script` is a common initialization script for Linux builds, not for the entire project. 

## how to build?

* if you need iso,

> run 
> ```
> cargo make iso
> ```

* if you need efi,

> run 
> ```
> cargo build
> ```

> [!WARNING]
> **Microcode Notice**: 
> Microcode is prepared during the `cargo make init_project` phase, but it is **not** automatically updated or downloaded during runtime by the OS. 
>
> If you need to manually refresh or fetch the latest microcode after the initial setup, use the following task:
> ```
> cargo make update_microcode
> ```

## What is the `log_viewer` git module?
> Official log viewer.
> 
> If you want to run (in native Linux / GUI support version WSL)
>
> ```
> cargo run
> ```
