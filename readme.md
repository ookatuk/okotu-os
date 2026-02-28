* loading screen in **before** `exit_boot_services`

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
