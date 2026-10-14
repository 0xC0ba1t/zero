# pong: you (left) against the computer (right), first to 5 wins
# keys: w / s or the arrow keys move your paddle, q quits

let top = 2
let bottom = 22
let height = 5
let left_x = 3
let right_x = 76

let py = 10
let ay = 10
let bx = 40
let by = 12
let dx = 1
let dy = 0
let i = 0
let k = 0
let frame = 0
let delay = 45
let running = 1

sub draw_field
  cls
  color 8
  i = 0
  while i < WIDTH
    plot i, 1, 205
    plot i, 23, 205
    i = i + 1
  end
  i = top
  while i <= bottom
    if i % 2 == 0
      plot 40, i, 250
    end
    i = i + 1
  end
  text 24, 24, "w/s or arrows: move   q: quit"
end

sub draw_paddles
  color 11
  i = 0
  while i < height
    plot left_x, py + i, 219
    plot right_x, ay + i, 219
    i = i + 1
  end
end

sub erase_paddles
  i = 0
  while i < height
    plot left_x, py + i, ' '
    plot right_x, ay + i, ' '
    i = i + 1
  end
end

sub erase_ball
  if bx == 40 && by % 2 == 0
    color 8
    plot bx, by, 250
  else
    plot bx, by, ' '
  end
end

sub draw_ball
  color 14
  plot bx, by, 'O'
end

call draw_field
text 29, 12, "press any key to start"
k = getkey()
call draw_field
call draw_paddles
call draw_ball

while running
  call erase_paddles
  call erase_ball

  # your paddle
  k = key()
  if k == 'q' || k == ESC
    running = 0
  end
  if k == 'w' || k == 'W' || k == UP
    py = py - 2
  end
  if k == 's' || k == 'S' || k == DOWN
    py = py + 2
  end
  py = max(py, top)
  py = min(py, bottom - height + 1)

  call draw_paddles
  call draw_ball
  frame = frame + 1
  sleep delay
end

cls
