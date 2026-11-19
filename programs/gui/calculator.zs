# A calculator for the zero desktop. Click the keys or type: digits, + - * /,
# Enter for =, Backspace, Esc to clear. Numbers keep three decimals.
window(300, 460, "Calculator")

SCALE = 1000
shown = "0"
acc = 0
op = ""
fresh = true
keys = [["AC", "+/-", "%", "/"], ["7", "8", "9", "*"], ["4", "5", "6", "-"], ["1", "2", "3", "+"], ["0", ".", "="]]
pressed = ""
pressed_at = 0

# text -> fixed point (value * 1000)
func parse(t)
  neg = false
  if sub(t, 0, 1) == "-"
    neg = true
    t = sub(t, 1)
  end
  whole = 0
  frac = 0
  digits = 0
  seen_dot = false
  for ch in t
    if ch == "."
      seen_dot = true
    elif seen_dot
      if digits < 3
        frac = frac * 10 + num(ch)
        digits = digits + 1
      end
    else
      whole = whole * 10 + num(ch)
    end
  end
  while digits < 3
    frac = frac * 10
    digits = digits + 1
  end
  v = whole * SCALE + frac
  if neg
    return -v
  end
  return v
end

# fixed point -> text without trailing zeros
func fmt(v)
  sign = ""
  if v < 0
    sign = "-"
    v = -v
  end
  whole = v / SCALE
  frac = v % SCALE
  if frac == 0
    return sign + str(whole)
  end
  f = str(frac + SCALE)
  f = sub(f, 1)
  while sub(f, len(f) - 1, 1) == "0"
    f = sub(f, 0, len(f) - 1)
  end
  return sign + str(whole) + "." + f
end

func apply(a, o, b)
  if o == "+"
    return a + b
  elif o == "-"
    return a - b
  elif o == "*"
    return a * b / SCALE
  elif o == "/"
    if b == 0
      return 0
    end
    return a * SCALE / b
  end
  return b
end

func press(k)
  global shown, acc, op, fresh, pressed, pressed_at
  pressed = k
  pressed_at = ticks()
  sound("click")
  if k == "AC"
    shown = "0"
    acc = 0
    op = ""
    fresh = true
  elif k == "+/-"
    if sub(shown, 0, 1) == "-"
      shown = sub(shown, 1)
    elif shown != "0"
      shown = "-" + shown
    end
  elif k == "%"
    shown = fmt(parse(shown) / 100)
  elif k == "+" or k == "-" or k == "*" or k == "/"
    if op != "" and not fresh
      acc = apply(acc, op, parse(shown))
      shown = fmt(acc)
    else
      acc = parse(shown)
    end
    op = k
    fresh = true
  elif k == "="
    if op != ""
      acc = apply(acc, op, parse(shown))
      shown = fmt(acc)
      op = ""
      fresh = true
    end
  elif k == "."
    if fresh
      shown = "0."
      fresh = false
    elif find(shown, ".") < 0
      shown = shown + "."
    end
  else
    if fresh or shown == "0"
      shown = k
      fresh = false
    elif len(shown) < 14
      shown = shown + k
    end
  end
end

while frame()
  W = width()
  H = height()
  clear(rgb(28, 28, 30))
  # the display
  size = 32
  if len(shown) > 10
    size = 24
  end
  textr(W - 22, 70, shown, "white", size, true)
  if op != ""
    text(22, 82, op, rgb(255, 159, 10), 20, true)
  end
  # keyboard
  k = key()
  while k != 0
    if k >= '0' and k <= '9'
      press(chr(k))
    elif k == '+' or k == '-' or k == '*' or k == '/' or k == '.' or k == '%'
      press(chr(k))
    elif k == ENTER or k == '='
      press("=")
    elif k == ESC
      press("AC")
    elif k == 8
      if len(shown) > 1
        shown = sub(shown, 0, len(shown) - 1)
      else
        shown = "0"
      end
    end
    k = key()
  end
  # the keys
  top = 140
  gap = 10
  kw = (W - 5 * gap) / 4
  kh = (H - top - 6 * gap) / 5
  for r = 0 to 4
    row = keys[r]
    x = gap
    for k2 in row
      w2 = kw
      if k2 == "0"
        w2 = kw * 2 + gap
      end
      y = top + r * (kh + gap)
      col = rgb(51, 51, 54)
      fg = "white"
      if r == 0 and k2 != "/"
        col = rgb(165, 165, 168)
        fg = "black"
      end
      if k2 == "/" or k2 == "*" or k2 == "-" or k2 == "+" or k2 == "="
        col = rgb(255, 159, 10)
        if op == k2 and fresh
          col = "white"
          fg = rgb(255, 159, 10)
        end
      end
      hover = mousex() >= x and mousex() < x + w2 and mousey() >= y and mousey() < y + kh
      if hover
        col = mix(col, "white", 40)
        cursor("hand")
      end
      if pressed == k2 and ticks() - pressed_at < 120
        col = mix(col, "white", 110)
      end
      rrect(x, y, w2, kh, kh / 2, col)
      center(x + w2 / 2, y + kh / 2 - 13, k2, fg, 24)
      if hover and clicked()
        press(k2)
      end
      x = x + w2 + gap
    end
  end
end
