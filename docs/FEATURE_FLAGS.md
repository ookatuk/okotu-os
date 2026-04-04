# Feature List
> [!note]
> Here you can see the flags that are officially intended for use.

> Other flags are also available, but they tend to be niche.

## Examples
* [x] default
> description here.
* [ ] option
> ...

## Checks
* [x] enable_essential_safety_checks / enable_required_safety_checks
> Basic checks.\
> `essential` may vary depending on the environment.\
> `required`
> are basic checks.

> [!important]
> At the hardware level, within the expected range of this OS,
>
> If you are certain that nothing will be broken and you are certain that nothing will be broken, then disable it.

> [!note]
> This also includes ACPI table checks.

* [x] enable_normal_safety_checks
> General OS checks

> [!note]
> For example, placing a specific string at the beginning before releasing (requires use in conjunction with stack checks described later)
* [ ] enable_overprotective_safety_checks
> For untrusted code/debug

> [!tip]
> In most cases, using this in conjunction with debug_outputs is effective.

* [x] enable_stack_checks
> Enables stack canaries.\
> Nothing else of note.

* [x] enable_syscall_arg_checks
> Performs system call checks.

> [!important]
> The kernel panics if the application provides incorrect input.

## Debugging related
* [ ] enable_debug_level_outputs
> Enables `debug` level UART output.

* [ ] enable_lldb_debug
> Uses `int3` before startup.

* [ ] disable_panic_restarts
> Enables automatic restart in case of panic.

## Logging Related
* [x] enable_uart_outputs
> Enables UART output.
> Disabling this requires viewing from within the OS or through memory, but it is faster.

* [x] enable_log
> Enables logging.\
> If disabled, only UART output is performed if the above option is enabled.

## UX
* [ ] enable_error_location_caller
> When an error occurs, information about the source code where the error originated is included.

* [x] include_boot_option_to_memcheck
> Introduces built-in memory checking to boot options.

> [!important]
> `enable_boot_option`, which is explained in the UI section, is enabled as a dependency.

## UI
* [x] enable_boot_option
> Enables boot options.

> [!note]
> Basically,
>
> If there are any options with `include_boot_option_to_...`, they will be automatically enabled.

* [ ] enable_ligatures
> Enables ligatures by default.

> [!note]
> Instead of loading them from a file later, the necessary code is embedded.