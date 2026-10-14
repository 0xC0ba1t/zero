# zs, the zero script language

A tiny language for writing programs that run inside the kernel. Scripts are
compiled to a flat list of statements and executed by a small virtual machine.

Run built-in programs with `run <name>` (see `programs`), one-liners with
`exec "..."` and single expressions with `eval 1 + 2`. Press Ctrl+C to stop a
running script.

## Example

```
# count to three
let i = 1
while i <= 3
  print "i is ", i
  i = i + 1
end
```

## Values and variables

There is exactly one type: a 32-bit signed integer. Variables are created by
assigning to them and are 0 until then. `let x = 1` and `x = 1` are the same.
Arithmetic wraps around instead of failing.

Predefined variables: `UP` (256) and `DOWN` (257) for the arrow keys, `ENTER`
(10), `ESC` (27), `WIDTH` (80) and `HEIGHT` (25) of the screen.

Character literals such as `'q'` are numbers (the character code), and `;`
separates statements on one line. `#` starts a comment.

## Expressions

From the loosest to the tightest binding:

| operators | meaning |
| --------- | ------- |
| `\|\|` | or (right side only evaluated if needed) |
| `&&` | and (same) |
| `== != < <= > >=` | comparison, gives 1 or 0 |
| `+ -` | add, subtract |
| `* / %` | multiply, divide, remainder (division truncates towards zero) |
| `- !` | negate, not |

Dividing by zero stops the script with an error. Zero is false, everything
else is true.

## Functions

| function | result |
| -------- | ------ |
| `rand(n)` | a random number from 0 to n-1 |
| `abs(x)`, `min(a, b)`, `max(a, b)` | what they say |
| `key()` | the latest key pressed since the last call, or 0 |
| `getkey()` | waits for a key and returns it |
| `ticks()` | milliseconds since boot |

## Statements

| statement | what it does |
| --------- | ------------ |
| `let x = expr` / `x = expr` | assign |
| `print a, b, ...` | print strings and numbers on one line |
| `if cond` ... `else` ... `end` | conditional (`else` is optional) |
| `while cond` ... `end` | loop |
| `sub name` ... `end` | define a sub (no arguments, variables are global) |
| `call name` | run a sub; subs may be defined after the call |
| `return` / `exit` | leave the sub / stop the program |

## Screen

| statement | what it does |
| --------- | ------------ |
| `cls` | clear the screen |
| `plot x, y, ch` | draw a character (code page 437 code) at column x, row y |
| `text x, y, a, b, ...` | draw strings and numbers at a position |
| `color n` | set the drawing color, 0 to 15 |
| `sleep ms` | wait |

The screen is 80 columns by 25 rows, with 0, 0 at the top left. Drawing
outside the screen is ignored.
