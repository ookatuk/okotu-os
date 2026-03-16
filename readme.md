## Index
- [Features](#features)
- [Build/Debug Dependencies](#builddebug-dependencies)
- [How to Init](#how-to-init)
- [How to Build](#how-to-build)
- [What is the `log_viewer` git module?](#what-is-the-logviewer-git-module)

## Features
* Loading screen **before** `exit_boot_services`
* Deep Color (10-bit) Support
* **True Type Font(ttf)** support

### Enable/disable features during build

#### Runtime checks
* [x] Essentials
* [x] Normal
* [x] Overprotective
* [x] **Boot options**: Memory check, boot, and shutdown.
* [x] **Full memory check**: Built-in check (patterns: addr, 0x00, 0xff, 0x55, 0xAA).
* [ ] **Overprotective**: Built-in but disabled by default.
* [ ] **Ligatures**: Powered by `rustybizz`. Available both before and after `exit_boot_services`.
* [x] **UART and more!**: See `Cargo.toml` for details.

## Build/Debug Dependencies

* qemu-system-x86_64
* xorriso
* mkfs.msdos(dosfstools)
* ovmf

## How to Init?

> [!IMPORTANT]
> **Windows Compatibility & Environment**
> While we are currently planning to expand Windows support (including the creation of `.bat` files), the environment is still under development.
> **We strongly recommend using WSL2** for a stable and supported build environment at this time.

> [!NOTE]
> *Build System Internals*
> Most cargo make tasks are simple wrappers around the scripts in `scripts/`.
> You can achieve the same results by manually executing the corresponding script file (just avoid those starting with `internal_`).

1. Install `cargo-make`
> Run:
> ```bash
> cargo install cargo-make
> ```

2. Init project
> ```bash
> cargo make init_project
> # or
> ./init.(sh/bat)
> ```

> [!TIP]
> `scripts/internal_init_script` is a common initialization script for Linux builds, not for the entire project.

## How to Build?

* If you need ISO:
> Run:
> ```bash
> cargo make iso
> ```

* If you need EFI:
> Run:
> ```bash
> cargo build
> ```

> [!WARNING]
> **Microcode Notice**:
> Microcode is prepared during the `cargo make init_project` phase, but it is **not** automatically updated or downloaded during runtime by the OS.
>
> If you need to manually refresh or fetch the latest microcode after the initial setup, use the following task:
> ```bash
> cargo make update_microcode
> ```

## What is the `log_viewer` git module?
> This is the official log viewer.
>
> Please note that this module is hosted in a separate repository. To use it, you must **request access** to the repository or **request the pre-compiled binary** from the maintainers.
>
> Once obtained, place the executable in the following directory:
> `bin/log_viewer`
>
> To run it (requires native Linux or WSL with GUI support):
> ```bash
> cargo run
> ```