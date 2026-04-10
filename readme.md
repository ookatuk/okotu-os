# okots (Okots Kernel Open Test System)
> An OS with many dependencies, seemingly meticulously designed but bit really.

> [!IMPORTANT]
> because I'm inexperienced, the commit size is huge, so I can't guarantee proper development.

> [!TIP]
> Please note that many comments and commits are in Japanese.
Also, there are cases where there are no comments at all.

This readme is full of jokes,

As this project is still in an early stage, commits may be large and development may be somewhat unstable.
# Description

for `x86_64-v2` OS.

I don't know its purpose!

I don't know if it's compatible with common operating systems!

Windows might not be(very not) compatible!

We plan to specify the app version in YAML instead of XML!

> [!NOTE]
> It's possible that compatibility with Linux will be handled through a special layer.

## Build/Debug Dependencies
### Build
* xorriso
* mkfs.msdos(dosfstools)
* nasm
* sccache
### Debug
* qemu-system-x86_64
* ovmf
* gdb-multiarch

## How to Init?

> [!IMPORTANT]
> **Windows Compatibility & Environment**
> While we are currently planning to expand Windows support (including the creation of `.bat` files), the environment is still under development.
> **We strongly recommend using WSL2** for a stable and supported build environment at this time.

> [!NOTE]
> *Build System Internals*
> Most cargo make tasks simply wrap scripts located in `scripts/` or add options to a single command.\
> You can achieve the same result by directly executing the corresponding script file (but avoid files starting with `internal_`).

1. Install `cargo-make`
> Run:
> ```bash
> cargo install cargo-make
> ```

2. Init project
> ```bash
> cargo make init_project
> # or
> ./init.sh
> # or
> ./init.bat
> ```

> [!TIP]
> `scripts/internal_init_script` is a common initialization script for Linux builds, not for the entire project.

## How to Build?

> [!IMPORTANT]
> Use sccache, or your SSD will scream.
>
> Each full build writes about 1.2GiB of data.
>
> To save your storage (and your sanity), we strongly recommend using sccache and moving the target folder to a temporary partition.
>
> We are currently seeking suggestions on how to address this issue.

> [!WARNING]
> **Microcode Notice**:
> Microcode is prepared during the `cargo make init_project` phase, but it is **not** automatically updated or downloaded during runtime by the OS.
>
> If you need to manually refresh or fetch the latest microcode after the initial setup, use the following task:
> ```bash
> cargo make update_microcode
> ```

* If you need ISO:
> Run:
> ```bash
> cargo make iso
> ```

* If you need EFI:
> Run:
> ```bash
> cargo make build
> ```

## What is the `log_viewer`?
> This is the official log viewer.
>
> Please note that this module is hosted in a separate repository. To use it, you must **request access** to the repository or **request the pre-compiled binary** from the maintainers.
>
> Once obtained, place the executable in the following directory:
> `bin/log_viewer`
>
> To run it (requires native Linux or WSL with GUI support):
> ```bash
> cargo make run
> ```
> I just don't want to regret because my code is so bad

> [!TIP] 
> ### How to run test?
> This OS can use `cargo make test`.\
> However, because it runs as an application and not as an operating system, some parts cannot be tested.

### ---
Are there too many dependencies?

Are the development environment requirements too stringent?

If there's no GUI, what's the point of using ligatures, logically speaking?

If you think there are too many dependencies... you're right. Too bad!

**I'm making this (a tiny bit) extravagant!**
