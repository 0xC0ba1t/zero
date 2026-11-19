# snake: eat the food, don't hit yourself
# arrows or w/a/s/d steer, p pauses, q quits.
#   easy   - slow, and the walls wrap around
#   normal - walls are solid
#   hard   - fast, solid walls, rocks in the way, and it speeds up as you grow

dim sx[836]
dim sy[836]
dim occ[836]

let level = 1
let running = 1
let choosing = 1
let alive = 1
let k = 0
let i = 0
let n = 0
let hd = 0
let tl = 0
let len = 0
let dx = 1
let dy = 0
let ndx = 1
let ndy = 0
let fx = 0
let fy = 0
let nx = 0
let ny = 0
let score = 0
let best = 0
let step = 120
let minstep = 120
let t0 = 0
let tnext = 0
let cx = 0
let cy = 0
let ate = 0

sub pick_level
  choosing = 1
  while choosing
    cls
    color 9
    text 35, 3, "S N A K E"
    color 8
    text 25, 5, "choose a difficulty: up/down, enter"
    i = 0
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
end

# a cell of the 38x22 board, in the current colour
sub cell
  rect 16 + cx * 16 + 1, 32 + cy * 16 + 1, 14, 14
end

sub draw_hud
  color 0
  rect 0, 0, 640, 24
  color 15
  text 2, 0, "score ", score, "   best ", best
  color 8
  if level == 0
    text 68, 0, "EASY"
  end
  if level == 1
    text 66, 0, "NORMAL"
  end
  if level == 2
    text 68, 0, "HARD"
  end
end

sub draw_food
  cx = fx
  cy = fy
  color 12
  disc 16 + fx * 16 + 8, 32 + fy * 16 + 8, 6
  color 10
  rect 16 + fx * 16 + 7, 32 + fy * 16 + 1, 2, 4
end

sub new_food
  fx = rand(38)
  fy = rand(22)
  while occ[fy * 38 + fx] != 0
    fx = (fx + 7) % 38
    if occ[fy * 38 + fx] != 0
      fy = (fy + 3) % 22
    end
  end
  call draw_food
end

sub draw_board
  cls
  color 8
  text 20, 24, "arrows / wasd: steer    p: pause    q: quit"
  rect 14, 30, 612, 2
  rect 14, 384, 612, 2
  rect 14, 30, 2, 356
  rect 624, 30, 2, 356
end

sub reset_game
  i = 0
  while i < 836
    occ[i] = 0
    i = i + 1
  end
  call draw_board
  # rocks on hard
  if level == 2
    n = 0
    while n < 9
      cx = 4 + rand(30)
      cy = 2 + rand(18)
      if occ[cy * 38 + cx] == 0 && abs(cx - 19) + abs(cy - 11) > 4
        occ[cy * 38 + cx] = 2
        color 8
        call cell
        n = n + 1
      end
    end
  end
  hd = 2
  tl = 0
  len = 3
  i = 0
  while i < 3
    sx[i] = 17 + i
    sy[i] = 11
    occ[11 * 38 + 17 + i] = 1
    color 10
    cx = 17 + i
    cy = 11
    call cell
    i = i + 1
  end
  dx = 1
  dy = 0
  ndx = 1
  ndy = 0
  score = 0
  alive = 1
  if level == 0
    step = 150
  end
  if level == 1
    step = 100
  end
  if level == 2
    step = 70
  end
  minstep = 45
  call new_food
  call draw_hud
end

sub steer
  k = key()
  if (k == UP || k == 'w') && dy == 0
    ndx = 0
    ndy = -1
  end
  if (k == DOWN || k == 's') && dy == 0
    ndx = 0
    ndy = 1
  end
  if (k == LEFT || k == 'a') && dx == 0
    ndx = -1
    ndy = 0
  end
  if (k == RIGHT || k == 'd') && dx == 0
    ndx = 1
    ndy = 0
  end
  if k == 'q' || k == ESC
    running = 0
    alive = 0
  end
  if k == 'p'
    color 15
    text 35, 12, "PAUSED"
    k = getkey()
    color 0
    rect 280, 192, 80, 16
    call draw_food
  end
end

# move one step
sub advance
  dx = ndx
  dy = ndy
  nx = sx[hd] + dx
  ny = sy[hd] + dy
  if level == 0
    nx = (nx + 38) % 38
    ny = (ny + 22) % 22
  end
  if nx < 0 || nx >= 38 || ny < 0 || ny >= 22
    alive = 0
    emote 5
    return
  end
  if occ[ny * 38 + nx] != 0
    alive = 0
    emote 5
    return
  end
  ate = 0
  if nx == fx && ny == fy
    ate = 1
  end
  # the head moves in
  hd = (hd + 1) % 836
  sx[hd] = nx
  sy[hd] = ny
  occ[ny * 38 + nx] = 1
  # the head of the snake is brighter
  color 11
  cx = nx
  cy = ny
  call cell
  color 10
  let prev = (hd + 835) % 836
  cx = sx[prev]
  cy = sy[prev]
  call cell
  if ate
    emote 1
    len = len + 1
    score = score + 10 * (level + 1)
    best = max(best, score)
    if level == 2
      step = max(step - 2, minstep)
    end
    call new_food
    call draw_hud
  else
    # the tail moves out
    occ[sy[tl] * 38 + sx[tl]] = 0
    color 0
    cx = sx[tl]
    cy = sy[tl]
    call cell
    tl = (tl + 1) % 836
  end
end

call pick_level
while running
  call reset_game
  tnext = ticks() + 500
  while alive
    t0 = ticks()
    call steer
    if alive && t0 >= tnext
      call advance
      tnext = t0 + step
    end
    sleep 8
  end
  if running
    color 12
    text 33, 11, "GAME OVER"
    color 15
    text 24, 13, "enter: again   q: quit"
    k = 0
    while k != ENTER && k != 'q' && k != ESC && k != SPACE
      k = getkey()
    end
    if k != ENTER && k != SPACE
      running = 0
    end
  end
end
cls
color 15
print "Thanks for playing snake! best score: ", best
