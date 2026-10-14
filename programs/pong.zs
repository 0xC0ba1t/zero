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
let off = 0
let i = 0
let k = 0
let frame = 0
let delay = 45
let you = 0
let cpu = 0
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
  call draw_score
end

sub draw_score
  color 15
  text 34, 0, you, " : ", cpu
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

# the ball bounces differently depending on where it hits the paddle
sub deflect
  if off < 2
    dy = -1
  end
  if off == 2
    dy = 0
  end
  if off > 2
    dy = 1
  end
end

sub after_point
  call draw_score
  bx = 40
  by = 12
  sleep 600
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

  # the computer's paddle: slower than the ball, so it can be beaten
  if frame % 3 != 0 && dx > 0
    if by < ay + 2
      ay = ay - 1
    end
    if by > ay + 2
      ay = ay + 1
    end
  end
  ay = max(ay, top)
  ay = min(ay, bottom - height + 1)

  # the ball
  bx = bx + dx
  by = by + dy
  by = max(by, top)
  by = min(by, bottom)
  if by <= top && dy < 0
    dy = 1
  end
  if by >= bottom && dy > 0
    dy = -1
  end

  if bx == left_x + 1 && dx < 0
    off = by - py
    if off >= 0 && off < height
      dx = 1
      call deflect
    end
  end
  if bx == right_x - 1 && dx > 0
    off = by - ay
    if off >= 0 && off < height
      dx = -1
      call deflect
    end
  end

  if bx <= 0
    cpu = cpu + 1
    dx = -1
    call after_point
  end
  if bx >= WIDTH - 1
    you = you + 1
    dx = 1
    call after_point
  end

  call draw_paddles
  call draw_ball
  frame = frame + 1
  sleep delay
end

cls
