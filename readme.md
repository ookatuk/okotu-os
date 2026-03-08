## features
* loading screen in **before** `exit_boot_services`
* before exit boot services and later HDR(In UEFI mode, need GOP Bitmask.)
* **True Type Font(ttf)** and **ligatures**  support
* Enable/disable features during build

> runtime check(essentials, normal, overprotective)\
> boot option(memory check, boot, shutdown)\
> build in full memory check(addr, 0x00, 0xff, 0x55,0xAA)\
> ligatures (powered by rustybizz. available both before and after exit_boot_services)\
> **UART and more!**(Look at cargo.toml)

## build/debug dependencies

* qemu-system-x86_64
* xorriso
* mkfs.msdos(dosfstools)
* ovmf

## how to build?

> (if you're using Windows, please install and use `WSL2`)\
> (Although there are some bat programs available,\
> I have not been verified or do not exist at all,
> so Windows does not fully support them.)

1. install `cargo-make`

> run `cargo install cargo-make`

2. if you need iso,

> run `cargo make iso`

4. if you need efi,

> run `cargo make build`\
> or
> run `cargo build`

5. if you need run and use Official Log Viewer,(in native Linux / GUI support version WSL)

> run `cargo make run`
> or run `cargo run`
