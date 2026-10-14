# zero
Tiny Rust OS

## Shell

After booting, `zero` drops into a tiny shell on the VGA console:

```
zero shell v0.2.0 - type `help` for a list of commands
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

### Keys

| key | action |
| --- | ------ |
| Backspace | erase the last character |
| Up / Down | browse the history |
| Tab | complete the command name |
| Ctrl+C | abandon the current line |
| Ctrl+L | clear the screen |
| Ctrl+U | erase the whole line |

Lines are limited to 72 characters so they never wrap onto a second row.

## Scripts

`zero` has a tiny scripting language, zs. See [docs/zs.md](docs/zs.md).

```
zero> eval 2 + 3 * 4
14
zero> programs
zero> run pong
```

| command | what it does |
| ------- | ------------ |
| `run <program>` | run a built-in script |
| `programs` | list the built-in scripts |
| `exec <script>` | run a one-line script, `;` separates statements |
| `eval <expression>` | print the value of an expression |
| `src <program>` | show the source of a built-in script |

Ctrl+C stops a running script.
