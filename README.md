# zero
Tiny Rust OS

## Shell

After booting, `zero` drops into a tiny shell on the VGA console:

```
zero shell v0.1.0 - type `help` for a list of commands
zero> echo hello
hello
```

Run it with `make run` (needs QEMU and `bootimage`).

### Commands

| command | what it does |
| ------- | ------------ |
| `help [command]` | list commands, or show how to use one |
| `echo [text...]` | print its arguments (double quotes group words) |
| `clear` | clear the screen |
| `version` | print the kernel name and version |
| `uptime` | show the time since boot |
| `mem` | show heap usage |
| `color [name]` | set the text color, or list the colors |
| `history` | list the lines you entered |
| `breakpoint` | trigger a breakpoint exception |
| `panic` | panic on purpose |
| `reboot` | restart the machine |
| `shutdown` | power off (virtual machines only) |
