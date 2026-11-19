# calculator: whole numbers
# type digits and + - * / directly, enter or = for the result, backspace to
# delete a digit, c to clear. Or move over the keypad with the arrows (or
# w/a/s/d) and press enter on a key. q quits.

dim kc[20]
dim kw[20]

let acc = 0
let cur = 0
let op = 0
let entering = 0
let fresh = 1
let err = 0
let k = 0
let cr = 1
let cc = 0
let running = 1
let i = 0
let r = 0
let c = 0
let n = 0
let digits = 0
let v = 0
let x = 0
let y = 0
let w = 0
let sel = 0
let flash = 0
let shown = 0

# what each keypad button does, row by row (4 columns, 5 rows)
sub init_keys
  kc[0] = 'c' ; kc[1] = 8 ; kc[2] = 'n' ; kc[3] = '/'
  kc[4] = '7' ; kc[5] = '8' ; kc[6] = '9' ; kc[7] = '*'
  kc[8] = '4' ; kc[9] = '5' ; kc[10] = '6' ; kc[11] = '-'
  kc[12] = '1' ; kc[13] = '2' ; kc[14] = '3' ; kc[15] = '+'
  kc[16] = '0' ; kc[17] = '0' ; kc[18] = '=' ; kc[19] = '='
end

# apply the pending operator: acc = acc op cur
sub compute
  if op == '+'
    acc = acc + cur
  end
  if op == '-'
    acc = acc - cur
  end
  if op == '*'
    acc = acc * cur
  end
  if op == '/'
    if cur == 0
      err = 1
    else
      acc = acc / cur
    end
  end
end

# a key press: a digit, an operator, '=', 'c' (clear), 8 (backspace) or 'n' (negate)
sub press
  if k == 'c'
    acc = 0
    cur = 0
    op = 0
    entering = 0
    fresh = 1
    err = 0
    return
  end
  if err
    return
  end
  if k >= '0' && k <= '9'
    if fresh
      cur = 0
      fresh = 0
    end
    if abs(cur) < 100000000
      if cur < 0
        cur = cur * 10 - (k - '0')
      else
        cur = cur * 10 + (k - '0')
      end
    end
    entering = 1
    return
  end
  if k == 8
    if entering
      cur = cur / 10
    end
    return
  end
  if k == 'n'
    cur = -cur
    if entering == 0
      acc = cur
    end
    return
  end
  if k == '+' || k == '-' || k == '*' || k == '/'
    if entering
      if op != 0
        call compute
      else
        acc = cur
      end
    end
    if err == 0
      cur = acc
      op = k
      entering = 0
      fresh = 1
    end
    return
  end
  if k == '=' || k == ENTER
    if op != 0 && (entering || fresh == 0)
      call compute
    else
      if entering
        acc = cur
      end
    end
    if err == 0
      cur = acc
      op = 0
      entering = 0
      fresh = 1
    end
  end
end

# the width of v in characters
sub measure
  digits = 1
  n = abs(v)
  while n >= 10
    n = n / 10
    digits = digits + 1
  end
  if v < 0
    digits = digits + 1
  end
end

sub draw_keys
  r = 0
  while r < 5
    c = 0
    while c < 4
      i = r * 4 + c
      w = 72
      x = 164 + c * 78
      y = 144 + r * 48
      # a wide button covers two cells: draw it once, from its left cell
      let draw = 1
      let selected = 0
      if r == 4
        w = 150
        if c == 1 || c == 3
          draw = 0
        end
        if r == cr && ((c == 0 && cc < 2) || (c == 2 && cc >= 2))
          selected = 1
        end
      else
        if r == cr && c == cc
          selected = 1
        end
      end
      if draw
        # the button
        color 8
        if selected
          color 9
        end
        rect x, y, w, 40
        color 0
        if flash && selected
          color 9
        end
        rect x + 2, y + 2, w - 4, 36
        color 7
        if kc[i] == '=' || kc[i] == '+' || kc[i] == '-' || kc[i] == '*' || kc[i] == '/'
          color 14
        end
        if kc[i] == 'c'
          color 12
        end
        if selected
          color 15
        end
        if flash && selected
          color 0
        end
        if kc[i] == 8
          text (x + w / 2) / 8 - 1, y / 16 + 1, "<-"
        else
          if kc[i] == 'n'
            text (x + w / 2) / 8 - 1, y / 16 + 1, "+-"
          else
            plot (x + w / 2) / 8, y / 16 + 1, kc[i]
          end
        end
      end
      c = c + 1
    end
    r = r + 1
  end
end

sub draw_display
  # the screen
  color 8
  rect 164, 40, 312, 96
  color 0
  rect 166, 42, 308, 92
  # the pending operation, small
  if op != 0 && err == 0
    v = acc
    call measure
    color 8
    text 55 - digits, 4, acc
    plot 57, 4, op
  end
  # the number
  color 15
  if err
    color 12
    text 40, 6, "can't divide by 0"
  else
    v = cur
    call measure
    text 59 - digits, 6, cur
  end
end

sub draw_all
  cls
  color 15
  text 36, 1, "CALCULATOR"
  color 8
  text 12, 24, "digits + - * /   enter: =   backspace: delete   c: clear   q: quit"
  call draw_display
  call draw_keys
end

call init_keys
call draw_all
while running
  k = getkey()
  if k == 'q' || k == ESC
    running = 0
  else
    sel = 0
    if k == UP || k == 'w'
      cr = max(cr - 1, 0)
    end
    if k == DOWN || k == 's'
      cr = min(cr + 1, 4)
    end
    if k == LEFT || k == 'a'
      cc = max(cc - 1, 0)
    end
    if k == RIGHT || k == 'd'
      cc = min(cc + 1, 3)
    end
    if k == ENTER && (cr != 4 || cc >= 0)
      # a press on the highlighted button
      k = kc[cr * 4 + cc]
      sel = 1
    end
    if (k >= '0' && k <= '9') || k == '+' || k == '-' || k == '*' || k == '/' || k == '=' || k == 'c' || k == 8 || k == 'n' || sel
      call press
    end
    flash = sel
    call draw_all
    if sel
      sleep 80
      flash = 0
      call draw_all
    end
  end
end
cls
color 15
print "Thanks for using the calculator!"
