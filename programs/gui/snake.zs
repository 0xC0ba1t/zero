# Snake for the zero desktop. Arrows or WASD steer, P pauses.
window(640, 520, "Snake")

COLS = 24
ROWS = 18
best = load("best", 0)
state = "menu"
speeds = [140, 95, 60]
names = ["Easy", "Normal", "Hard"]
level = 1
snake = []
dir = [1, 0]
nextdir = [1, 0]
apple = [0, 0]
score = 0
grow = 0
last_step = 0

func reset()
  global snake, score, last_step, state, grow
  snake = [[8, 9], [7, 9], [6, 9], [5, 9]]
  dir[0] = 1
  dir[1] = 0
  nextdir[0] = 1
  nextdir[1] = 0
  score = 0
  grow = 0
  last_step = ticks()
  place_apple()
  state = "play"
end

func place_apple()
  global apple
  while true
    apple = [rand(COLS), rand(ROWS)]
    hit = false
    for s in snake
      if s[0] == apple[0] and s[1] == apple[1]
        hit = true
      end
    end
    if not hit
      return
    end
  end
end

func step()
  global state, score, best, grow
  head = snake[0]
  nx = head[0] + nextdir[0]
  ny = head[1] + nextdir[1]
  dir[0] = nextdir[0]
  dir[1] = nextdir[1]
  dead = nx < 0 or ny < 0 or nx >= COLS or ny >= ROWS
  for s in snake
    if s[0] == nx and s[1] == ny
      dead = true
    end
  end
  if dead
    state = "over"
    emote(5)
    if score > best
      best = score
      save("best", best)
      say("new snake record: " + str(best) + "!")
    end
    return
  end
  insert(snake, 0, [nx, ny])
  if nx == apple[0] and ny == apple[1]
    score = score + 10 * (level + 1)
    grow = grow + 2
    emote(1)
    place_apple()
  end
  if grow > 0
    grow = grow - 1
  else
    pop(snake)
  end
end

func turn(dx, dy)
  if dx != -dir[0] or dy != -dir[1]
    nextdir[0] = dx
    nextdir[1] = dy
  end
end

while frame()
  if dark()
    clear(rgb(24, 26, 30))
  else
    clear(rgb(236, 240, 232))
  end
  cell = min((width() - 40) / COLS, (height() - 110) / ROWS)
  ox = (width() - cell * COLS) / 2
  oy = 70 + (height() - 90 - cell * ROWS) / 2
  text(ox, 24, "Score " + str(score), "text", 20, true)
  textr(ox + cell * COLS, 24, "Best " + str(best), "muted", 20)
  # the field: a checkerboard of two greens
  rrect(ox - 6, oy - 6, cell * COLS + 12, cell * ROWS + 12, 14, rgb(46, 125, 50))
  for y = 0 to ROWS - 1
    for x = 0 to COLS - 1
      if (x + y) % 2 == 0
        rect(ox + x * cell, oy + y * cell, cell, cell, rgb(102, 187, 106))
      else
        rect(ox + x * cell, oy + y * cell, cell, cell, rgb(129, 199, 132))
      end
    end
  end
  if state != "menu"
    ax = ox + apple[0] * cell + cell / 2
    ay = oy + apple[1] * cell + cell / 2
    circle(ax, ay + 1, cell / 2 - 1, rgb(229, 57, 53))
    circle(ax - cell / 6, ay - cell / 6, cell / 7, rgb(255, 138, 128))
    rrect(ax, ay - cell / 2, cell / 4, cell / 5, 2, rgb(56, 142, 60))
    n = len(snake)
    for i = n - 1 to 0 step -1
      s = snake[i]
      shade = 255 - i * 120 / max(n, 1)
      rrect(ox + s[0] * cell + 1, oy + s[1] * cell + 1, cell - 2, cell - 2, cell / 3, rgb(30, 90 + shade / 3, 200))
    end
    h = snake[0]
    hx = ox + h[0] * cell + cell / 2
    hy = oy + h[1] * cell + cell / 2
    circle(hx + dir[0] * cell / 5 - dir[1] * cell / 5, hy + dir[1] * cell / 5 - dir[0] * cell / 5, cell / 7, "white")
    circle(hx + dir[0] * cell / 5 + dir[1] * cell / 5, hy + dir[1] * cell / 5 + dir[0] * cell / 5, cell / 7, "white")
  end
  k = key()
  while k != 0
    if k == UP or k == 'w'
      turn(0, -1)
    elif k == DOWN or k == 's'
      turn(0, 1)
    elif k == LEFT or k == 'a'
      turn(-1, 0)
    elif k == RIGHT or k == 'd'
      turn(1, 0)
    elif k == 'p' or k == SPACE
      if state == "play"
        state = "pause"
      elif state == "pause"
        state = "play"
        last_step = ticks()
      end
    elif k == ENTER and state != "play"
      reset()
    end
    k = key()
  end
  if state == "play" and ticks() - last_step >= speeds[level]
    last_step = ticks()
    step()
  end
  cx = width() / 2
  cy = oy + cell * ROWS / 2
  if state == "menu" or state == "over"
    fade(ox - 6, oy - 6, cell * COLS + 12, cell * ROWS + 12, "black", 120)
    if state == "menu"
      center(cx, cy - 110, "SNAKE", "white", 32, true)
      center(cx, cy - 66, "eat the apples, don't bite yourself", "white", 16)
    else
      center(cx, cy - 110, "Game over", "white", 32, true)
      center(cx, cy - 66, "you scored " + str(score), "white", 16)
    end
    for i = 0 to 2
      if button(cx - 165 + i * 112, cy - 20, 100, 40, names[i], i == level)
        level = i
      end
    end
    if button(cx - 80, cy + 40, 160, 44, "Play", true)
      reset()
    end
  elif state == "pause"
    fade(ox - 6, oy - 6, cell * COLS + 12, cell * ROWS + 12, "black", 110)
    center(cx, cy - 20, "Paused", "white", 32, true)
    center(cx, cy + 22, "press P to go on", "white", 16)
  end
end
