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

Predefined variables: `UP` (256), `DOWN` (257), `LEFT` (258) and `RIGHT` (259) for the
arrow keys, `ENTER` (10), `ESC` (27), `SPACE` (32), and `WIDTH` (80) and `HEIGHT`
(25) of the character grid.

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
| `down(key)` | 1 while the key is held down right now (a letter, an arrow, `SPACE`, `ENTER`, `ESC`) |

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

## Arrays

Declare a fixed-size array of integers (all zero) with `dim`, anywhere in the
program, before it is used:

```
dim board[42]
board[3] = 1
print board[3] + board[4]
```

Indexing outside the array stops the program with an error. Subs have no
parameters or local variables, so recursive code keeps its locals in arrays
indexed by the recursion depth (see `tictactoe.zs`, which searches with minimax).

## Screen

| statement | what it does |
| --------- | ------------ |
| `cls` | clear the screen |
| `plot x, y, ch` | draw a character (code page 437 code) at column x, row y |
| `text x, y, a, b, ...` | draw strings and numbers at a position |
| `color n` | set the drawing color, 0 to 15 |
| `rect x, y, w, h` | fill a rectangle in the current color, in *pixels* (see below) |
| `disc x, y, r` | fill a circle in the current color, in pixels |
| `sleep ms` | wait (and show what has been drawn) |

The character grid is 80 columns by 25 rows, with 0, 0 at the top left. Drawing
outside the screen is ignored.

`rect` and `disc` use a finer coordinate system: the screen is 640 by 400
pixels (a character cell is 8 by 16 of them), scaled to whatever the real
resolution is. Because the real screen is usually bigger, positions can be
smooth: keep positions in fixed point (say, 4 or 8 times the pixel) and divide
when you draw. A frame loop with `down()` and `sleep` gives smooth motion, see
`pong.zs`. To erase something, draw it again in color 0.

## Adding a program

There is no file system yet. Put a `.zs` file in `programs/` and list it in
`src/script/programs.rs`, then rebuild.

## Pong

`run pong`: you are the left paddle. `w`/`s` or the arrow keys move, `q`
quits. First to five wins. Where the ball hits your paddle decides its angle.
