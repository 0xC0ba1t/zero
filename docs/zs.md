# ZeroScript

The scripting language of zero. The desktop's games, the calculator and your own
programs are written in it. Open a `.zs` file from Files (or run it from Notes:
File > Run as ZeroScript) and it gets a window of its own.

```
window(480, 300, "Hello")
n = 0
while frame()
  clear()
  center(width() / 2, 100, "Hello, zero!", "text", 32, true)
  if button(width() / 2 - 60, 170, 120, 40, "Clicked " + str(n), true)
    n += 1
    sound("click")
  end
end
```

`frame()` shows what you drew and waits for the next frame (about 60 a second). It
returns 0 when the window is closed, so `while frame()` is the usual main loop.

## The language

**Values:** whole numbers, text (`"hi"`), lists (`[1, 2, 3]`) and `nil`. `true` and
`false` are 1 and 0. Text can be joined with `+`, and `"ab" * 3` repeats it.
Comments start with `#`; `;` separates statements on one line.

**Variables** are created by assigning. Inside a `func`, variables are local; write
`global name` to use or change a global one.

```
x = 5
x += 2          # also -=  *=  /=  %=
names = ["ann", "bo"]
names[0] = "anna"
print names[-1]   # negative indexes count from the end
```

**Conditions and loops**

```
if a < b
  ...
elif a == b
  ...
else
  ...
end

while cond ... end
for i = 1 to 10 step 2 ... end
for item in list ... end      # also over text (letters) and numbers (0..n-1)
break    continue
```

Operators: `+ - * / %`, comparisons `== != < <= > >=`, and `and or not` (also `&& || !`).
`and`/`or` stop early. Dividing by zero is an error.

**Functions**

```
func area(w, h)
  return w * h
end
print area(3, 4)
```

Functions can call themselves. The old `sub name ... end` / `call name` style still works
(subs see only global variables).

## Standard library

| function | what it does |
| --- | --- |
| `len(x)` | length of text or a list |
| `str(x)`, `num(x)`, `chr(n)`, `ord(s)` | conversions |
| `upper(s)`, `lower(s)`, `trim(s)` | text case and spaces |
| `sub(s, start, len)` | part of a text; `find(s, part)` gives a position or -1 |
| `split(s, sep)`, `join(list, sep)` | text to list and back |
| `range(n)` / `range(a, b)` | a list of numbers |
| `push(l, v)`, `pop(l)`, `insert(l, i, v)`, `remove(l, i)`, `copy(l)`, `contains(l, v)` | lists |
| `abs min max clamp sign sqrt sin cos` | maths (`sin`/`cos` take degrees, give 1000 times the value) |
| `rand(n)` | a random number from 0 to n-1 |
| `ticks()` | milliseconds since boot |
| `type(x)` | `"number"`, `"text"`, `"list"` or `"nil"` |

## The desktop

Colours are a number (`rgb(r, g, b)` or `0xRRGGBB`) or a name: `"text"`, `"muted"`, `"bg"`,
`"panel"`, `"line"`, `"accent"`, `"white"`, `"black"`, `"red"`, `"green"`, `"blue"`,
`"yellow"`, `"orange"`, `"purple"`, `"pink"`, `"gray"`. Theme colours follow light and dark mode.

| window | |
| --- | --- |
| `window(w, h, title)` | set the size of the window's contents and its title |
| `title(text)` | change the title |
| `frame()` | show the picture, wait for the next frame; 0 when closed |
| `sleep(ms)` | wait, still showing frames |
| `width()`, `height()` | size of the window's contents |

| drawing | |
| --- | --- |
| `clear([colour])` | fill the window |
| `rect(x, y, w, h, c)`, `rrect(x, y, w, h, radius, c)` | rectangles, smooth rounded corners |
| `fade(x, y, w, h, c, alpha)` | translucent rectangle (alpha 0 to 255) |
| `outline(x, y, w, h, radius, c)` | a one pixel border |
| `circle(x, y, r, c)` | smooth circle |
| `line(x1, y1, x2, y2, c [, width])` | smooth line |
| `gradient(x, y, w, h, c1, c2 [, radius])` | top to bottom gradient |
| `shadow(x, y, w, h, radius, blur)` | soft shadow |
| `text(x, y, s [, c, size, bold])`, `center(...)`, `textr(...)` | text (left, centred, right aligned) |
| `textw(s [, size, bold])` | width of a text |
| `image(path, x, y [, w, h])` | draw a picture file (png, gif, bmp) |
| `rgb(r,g,b)`, `mix(c1, c2, 0..255)`, `theme(name)`, `dark()` | colours |

| input | |
| --- | --- |
| `mousex()`, `mousey()`, `mousedown()` | pointer position and left button |
| `clicked()`, `rclicked()` | a click happened this frame |
| `wheel()` | scroll wheel movement this frame |
| `key()` | next key pressed (0 if none); `UP DOWN LEFT RIGHT ENTER ESC SPACE` are constants |
| `keydown(k)` | 1 while a key (letter or arrow) is held |
| `button(x, y, w, h, label [, primary])` | draws a button, returns 1 when clicked |
| `cursor("hand" \| "text" \| "arrow")` | pointer shape |

| everything else | |
| --- | --- |
| `sound("click" \| "score" \| "hurt" \| "win" \| "lose" \| "danger")` | play a sound effect |
| `emote(1..5)` | tell Bit how the game goes (score, hurt, win, danger, lose) |
| `say(text)` | Bit says something |
| `notify(title, text)` | show a notification |
| `save(name, value)`, `load(name, default)` | keep a value between runs |
| `time()` | the clock as `hh:mm:ss` |
| `print a, b, ...` | print text (shown in the window if the program never draws) |

Programs that crash show the error and line number in their window.

## Examples

The built-in apps are in `programs/gui/`: `snake.zs`, `pong.zs`, `invaders.zs`,
`tictactoe.zs` (alpha-beta minimax), `connect4.zs`, `calculator.zs` and `bounce.zs`.
The old text-mode programs in `programs/` still run from the terminal with `run <name>`.
