# zs interactive calculator with grid keypad
# controls: w/a/s/d or arrows to move, ENTER or space to press, q to quit

let cx = 0
let cy = 0
let running = 1
let k = 0
let i = 0

let acc = 0
let current_val = 0
let op = 0
let has_input = 0
let error_state = 0

sub draw_keypad
  cls
  color 15
  text 32, 2, "ZS CALCULATOR"

  # Display screen box
  color 8
  i = 24
  while i <= 55
    plot i, 4, 196
    plot i, 8, 196
    i = i + 1
  end
  plot 23, 5, 186
  plot 23, 6, 186
  plot 23, 7, 186
  plot 56, 5, 186
  plot 56, 6, 186
  plot 56, 7, 186

  # Show value on display
  color 14
  if error_state == 1
    text 26, 6, "ERROR (DIV BY ZERO)"
  else
    if has_input == 1
      text 26, 6, current_val
    else
      text 26, 6, acc
    end
  end

  # Draw keypad buttons
  color 7
  text 32, 11, " 7   8   9   / "
  text 32, 14, " 4   5   6   * "
  text 32, 17, " 1   2   3   - "
  text 32, 20, " C   0   =   + "

  color 7
  text 18, 23, "w/a/s/d or arrows: move   ENTER: press   q: quit"
end

sub draw_cursor
  let bx = 32
  let by = 11
  if cx == 0
    bx = 32
  end
  if cx == 1
    bx = 38
  end
  if cx == 2
    bx = 44
  end
  if cx == 3
    bx = 50
  end

  if cy == 0
    by = 11
  end
  if cy == 1
    by = 14
  end
  if cy == 2
    by = 17
  end
  if cy == 3
    by = 20
  end

  color 11
  plot bx - 1, by, '['
  plot bx + 1, by, ']'
end

sub input_0
  if has_input == 0
    current_val = 0
    has_input = 1
  else
    current_val = current_val * 10
  end
end

sub input_1
  if has_input == 0
    current_val = 1
    has_input = 1
  else
    current_val = current_val * 10 + 1
  end
end

sub input_2
  if has_input == 0
    current_val = 2
    has_input = 1
  else
    current_val = current_val * 10 + 2
  end
end

sub input_3
  if has_input == 0
    current_val = 3
    has_input = 1
  else
    current_val = current_val * 10 + 3
  end
end

sub input_4
  if has_input == 0
    current_val = 4
    has_input = 1
  else
    current_val = current_val * 10 + 4
  end
end

sub input_5
  if has_input == 0
    current_val = 5
    has_input = 1
  else
    current_val = current_val * 10 + 5
  end
end

sub input_6
  if has_input == 0
    current_val = 6
    has_input = 1
  else
    current_val = current_val * 10 + 6
  end
end

sub input_7
  if has_input == 0
    current_val = 7
    has_input = 1
  else
    current_val = current_val * 10 + 7
  end
end

sub input_8
  if has_input == 0
    current_val = 8
    has_input = 1
  else
    current_val = current_val * 10 + 8
  end
end

sub input_9
  if has_input == 0
    current_val = 9
    has_input = 1
  else
    current_val = current_val * 10 + 9
  end
end

sub compute
  if op == 0
    if has_input == 1
      acc = current_val
    end
  end
  if op == 1
    acc = acc + current_val
  end
  if op == 2
    acc = acc - current_val
  end
  if op == 3
    acc = acc * current_val
  end
  if op == 4
    if current_val == 0
      error_state = 1
    else
      acc = acc / current_val
    end
  end
  current_val = acc
end

sub apply_op
  if has_input == 1
    call compute
  else
    acc = current_val
  end
  has_input = 0
end

sub press_button
  error_state = 0
  # Row 0: 7, 8, 9, /
  if cy == 0 && cx == 0
    call input_7
  end
  if cy == 0 && cx == 1
    call input_8
  end
  if cy == 0 && cx == 2
    call input_9
  end
  if cy == 0 && cx == 3
    call apply_op
    op = 4
  end

  # Row 1: 4, 5, 6, *
  if cy == 1 && cx == 0
    call input_4
  end
  if cy == 1 && cx == 1
    call input_5
  end
  if cy == 1 && cx == 2
    call input_6
  end
  if cy == 1 && cx == 3
    call apply_op
    op = 3
  end

  # Row 2: 1, 2, 3, -
  if cy == 2 && cx == 0
    call input_1
  end
  if cy == 2 && cx == 1
    call input_2
  end
  if cy == 2 && cx == 2
    call input_3
  end
  if cy == 2 && cx == 3
    call apply_op
    op = 2
  end

  # Row 3: C, 0, =, +
  if cy == 3 && cx == 0
    acc = 0
    current_val = 0
    op = 0
    has_input = 0
  end
  if cy == 3 && cx == 1
    call input_0
  end
  if cy == 3 && cx == 2
    call compute
    op = 0
    has_input = 0
  end
  if cy == 3 && cx == 3
    call apply_op
    op = 1
  end
end

while running
  call draw_keypad
  call draw_cursor

  k = getkey()
  if k == 'q' || k == ESC
    running = 0
  end
  if k == 'w' || k == 'W' || k == UP
    cy = cy - 1
  end
  if k == 's' || k == 'S' || k == DOWN
    cy = cy + 1
  end
  if k == 'a' || k == 'A'
    cx = cx - 1
  end
  if k == 'd' || k == 'D'
    cx = cx + 1
  end

  cx = max(cx, 0)
  cx = min(cx, 3)
  cy = max(cy, 0)
  cy = min(cy, 3)

  if k == ENTER || k == ' '
    call press_button
  end
end

cls
color 15
print "Calculator closed. Final value: ", acc