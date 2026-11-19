# pong: you (left) against the computer (right)
# w/s or the arrow keys move your paddle (hold them: the paddle glides), q quits.
# The screen is 640x400 pixels. Positions are kept in fixed point
# (paddles x4, ball x8) so everything moves smoothly.

let level = 1
let choosing = 1
let running = 1
let ps = 0
let cs = 0
let py = 672
let cy = 672
let bx = 2560
let by = 1600
let vx = 0
let vy = 0
let base = 36
let speed = 36
let cspeed = 15
let reach = 22
let aim = 0
let opy = 168
let ocy = 168
let obx = 320
let oby = 200
let k = 0
let t0 = 0
let rel = 0
let dir = 1
let over = 0

sub pick_level
  choosing = 1
  while choosing
    cls
    color 9
    text 36, 3, "P O N G"
    color 8
    text 25, 5, "choose a difficulty: up/down, enter"
    let i = 0
    while i < 3
      if i == level
        color 9
        rect 200, 112 + i * 64, 240, 44
        color 0
        rect 203, 115 + i * 64, 234, 38
        color 15
      else
        color 8
        rect 200, 112 + i * 64, 240, 44
        color 0
        rect 202, 114 + i * 64, 236, 40
        color 7
      end
      if i == 0
        text 38, 8, "EASY"
      end
      if i == 1
        text 37, 12, "NORMAL"
      end
      if i == 2
        text 38, 16, "HARD"
      end
      i = i + 1
    end
    k = getkey()
    if k == UP || k == 'w'
      level = (level + 2) % 3
    end
    if k == DOWN || k == 's'
      level = (level + 1) % 3
    end
    if k == ENTER || k == SPACE
      choosing = 0
    end
    if k == 'q' || k == ESC
      choosing = 0
      running = 0
    end
  end
  # what the level changes: how fast the ball goes, and how well the cpu plays
  if level == 0
    base = 28
    cspeed = 10
    reach = 46
  end
  if level == 1
    base = 36
    cspeed = 15
    reach = 22
  end
  if level == 2
    base = 44
    cspeed = 22
    reach = 6
  end
end

sub draw_chrome
  cls
  color 8
  rect 0, 28, 640, 2
  rect 0, 378, 640, 2
  text 1, 0, "you"
  text 76, 0, "cpu"
  color 8
  text 22, 24, "w/s or arrows: move    q: quit"
end

sub draw_score
  color 15
  text 36, 0, ps, " : ", cs, "  "
end

sub serve
  speed = base
  bx = 2560
  by = 1600
  vx = dir * speed
  vy = rand(21) - 10
  aim = rand(reach * 2 + 1) - reach
  # the dashes and the ball are redrawn every frame; wipe the old ball
  color 0
  disc obx, oby, 7
  color 15
  text 33, 12, "GET READY"
  sleep 700
  color 0
  rect 256, 192, 128, 16
end

sub bounce_left
  rel = by / 8 - (py / 4 + 32)
  speed = min(speed + 2, base + 22)
  vy = rel * speed / 60
  vx = speed - abs(vy) / 2
  bx = 38 * 8
  aim = rand(reach * 2 + 1) - reach
end

sub bounce_right
  rel = by / 8 - (cy / 4 + 32)
  speed = min(speed + 2, base + 22)
  vy = rel * speed / 60
  vx = -(speed - abs(vy) / 2)
  bx = 602 * 8
  aim = rand(reach * 2 + 1) - reach
end

sub draw_objects
  # erase where things were
  color 0
  rect 24, opy, 8, 64
  rect 608, ocy, 8, 64
  disc obx, oby, 7
  # the dashed centre line
  color 8
  let y = 36
  while y < 372
    rect 318, y, 4, 12
    y = y + 24
  end
  # paddles and ball
  opy = py / 4
  ocy = cy / 4
  obx = bx / 8
  oby = by / 8
  color 11
  rect 24, opy, 8, 64
  rect 608, ocy, 8, 64
  color 14
  disc obx, oby, 6
end

sub frame
  # your paddle: 6 pixels per frame while a key is held
  if down('w') || down(UP)
    py = py - 24
  end
  if down('s') || down(DOWN)
    py = py + 24
  end
  py = max(py, 128)
  py = min(py, 1248)

  # the computer follows the ball (on hard it never takes its eyes off it)
  if bx > 2560 || level == 2
    let target = by / 8 + aim - 32
    let here = cy / 4
    if here < target - 3
      cy = cy + cspeed
    end
    if here > target + 3
      cy = cy - cspeed
    end
  end
  cy = max(cy, 128)
  cy = min(cy, 1248)

  # the ball
  bx = bx + vx
  by = by + vy
  if by < 38 * 8
    by = 38 * 8
    vy = abs(vy)
  end
  if by > 372 * 8
    by = 372 * 8
    vy = -abs(vy)
  end
  if vx < 0 && bx / 8 < 38 && bx / 8 > 22
    if abs(by / 8 - (py / 4 + 32)) < 38
      call bounce_left
    end
  end
  if vx > 0 && bx / 8 > 602 && bx / 8 < 618
    if abs(by / 8 - (cy / 4 + 32)) < 38
      call bounce_right
    end
  end

  call draw_objects

  if bx < -80
    cs = cs + 1
    emote 2
    dir = -1
    call draw_score
    call serve
  end
  if bx > 5200
    ps = ps + 1
    emote 1
    dir = 1
    call draw_score
    call serve
  end
end

call pick_level
while running
  ps = 0
  cs = 0
  py = 672
  cy = 672
  over = 0
  call draw_chrome
  call draw_score
  dir = 1
  call serve
  while running && !over
    t0 = ticks()
    call frame
    k = key()
    if k == 'q' || k == ESC
      running = 0
    end
    if ps >= 7 || cs >= 7
      over = 1
      if ps >= 7
        emote 3
      else
        emote 5
      end
    end
    # one frame every 16 ms
    let wait = 16 - (ticks() - t0)
    if wait > 0
      sleep wait
    end
  end
  if running
    color 15
    if ps >= 7
      text 30, 11, "YOU WIN!   enter: again  q: quit"
    else
      text 28, 11, "CPU WINS   enter: again  q: quit"
    end
    k = 0
    while k != ENTER && k != 'q' && k != ESC
      k = getkey()
    end
    if k != ENTER
      running = 0
    end
  end
end
cls
color 15
print "Thanks for playing pong!"
