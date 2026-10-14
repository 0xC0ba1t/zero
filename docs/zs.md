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
