# Connect Four for the zero desktop. Click a column to drop a disc.
# The computer looks a few moves ahead.
window(620, 600, "Connect Four")

COLS = 7
ROWS = 6
grid = range(42)
for i = 0 to 41
  grid[i] = 0
end
turn = 1
winner = 0
falling = []
level = 1
names = ["Easy", "Normal", "Hard"]
depths = [0, 1, 2]
think_at = 0
wins = load("wins", 0)
winning = []

func at(c, r)
  return grid[r * COLS + c]
end

func drop_row(c)
  for r = ROWS - 1 to 0 step -1
    if grid[r * COLS + c] == 0
      return r
    end
  end
  return -1
end

func line_win(p)
  dirs = [[1, 0], [0, 1], [1, 1], [1, -1]]
  for r = 0 to ROWS - 1
    for c = 0 to COLS - 1
      if grid[r * COLS + c] == p
        for d in dirs
          ok = true
          cells = []
          for k = 0 to 3
            cc = c + d[0] * k
            rr = r + d[1] * k
            if cc < 0 or cc >= COLS or rr < 0 or rr >= ROWS
              ok = false
              break
            end
            if grid[rr * COLS + cc] != p
              ok = false
              break
            end
            push(cells, rr * COLS + cc)
          end
          if ok
            return cells
          end
        end
      end
    end
  end
  return []
end

# a quick judgement of the position for player 2
func evaluate()
  s = 0
  for r = 0 to ROWS - 1
    if grid[r * COLS + 3] == 2
      s = s + 3
    elif grid[r * COLS + 3] == 1
      s = s - 3
    end
  end
  return s
end

func search(depth, player)
  if len(line_win(2)) > 0
    return 1000 + depth
  end
  if len(line_win(1)) > 0
    return -1000 - depth
  end
  if depth == 0
    return evaluate()
  end
  best = -100000
  if player == 1
    best = 100000
  end
  moved = false
  for c in [3, 2, 4, 1, 5, 0, 6]
    r = drop_row(c)
    if r >= 0
      moved = true
      grid[r * COLS + c] = player
      v = search(depth - 1, 3 - player)
      grid[r * COLS + c] = 0
      if player == 2
        best = max(best, v)
      else
        best = min(best, v)
      end
    end
  end
  if not moved
    return 0
  end
  return best
end

func computer_column()
  if level == 0 and rand(100) < 50
    c = rand(COLS)
    while drop_row(c) < 0
      c = rand(COLS)
    end
    return c
  end
  best = -1000000
  pick = 3
  for c in [3, 2, 4, 1, 5, 0, 6]
    r = drop_row(c)
    if r >= 0
      grid[r * COLS + c] = 2
      v = search(depths[level], 1)
      grid[r * COLS + c] = 0
      if v > best
        best = v
        pick = c
      end
    end
  end
  return pick
end

func drop(c)
  global turn, winner, winning, wins
  r = drop_row(c)
  if r < 0
    return
  end
  grid[r * COLS + c] = turn
  push(falling, [r * COLS + c, ticks()])
  sound("click")
  winning = line_win(turn)
  if len(winning) > 0
    winner = turn
    if turn == 1
      wins = wins + 1
      save("wins", wins)
      emote(3)
    else
      emote(5)
    end
    return
  end
  full = true
  for c2 = 0 to COLS - 1
    if drop_row(c2) >= 0
      full = false
    end
  end
  if full
    winner = 3
    return
  end
  turn = 3 - turn
end

func restart()
  global turn, winner, winning, falling
  for i = 0 to 41
    grid[i] = 0
  end
  turn = 1
  winner = 0
  winning = []
  falling = []
end

while frame()
  W = width()
  H = height()
  clear("bg")
  cell = min((W - 40) / COLS, (H - 170) / ROWS)
  ox = (W - cell * COLS) / 2
  oy = 120
  for i = 0 to 2
    if button(W / 2 - 170 + i * 116, 14, 108, 34, names[i], i == level)
      level = i
      restart()
    end
  end
  msg = "Your turn (red)"
  if winner == 1
    msg = "You win!"
  elif winner == 2
    msg = "The computer wins"
  elif winner == 3
    msg = "A draw"
  elif turn == 2
    msg = "Thinking..."
  end
  center(W / 2, 64, msg, "text", 24, true)
  textr(W - 20, 70, "wins: " + str(wins), "muted", 16)
  # the column under the pointer
  hc = (mousex() - ox) / cell
  if winner == 0 and turn == 1 and mousex() >= ox and hc >= 0 and hc < COLS
    circle(ox + hc * cell + cell / 2, oy - 18, cell / 3, rgb(255, 69, 58))
    if clicked()
      drop(hc)
    end
  end
  # the board
  rrect(ox - 10, oy - 4, cell * COLS + 20, cell * ROWS + 14, 18, rgb(28, 82, 210))
  for r = 0 to ROWS - 1
    for c = 0 to COLS - 1
      i = r * COLS + c
      cx = ox + c * cell + cell / 2
      cy = oy + r * cell + cell / 2 + 4
      v = grid[i]
      # discs fall into place
      for f in falling
        if f[0] == i
          t = min(ticks() - f[1], 300)
          cy = oy - cell + (cy - oy + cell) * t * t / 90000
        end
      end
      circle(ox + c * cell + cell / 2, oy + r * cell + cell / 2 + 4, cell * 4 / 10, rgb(14, 40, 110))
      if v == 1
        circle(cx, cy, cell * 4 / 10, rgb(255, 69, 58))
        circle(cx - cell / 10, cy - cell / 10, cell / 8, rgb(255, 130, 120))
      elif v == 2
        circle(cx, cy, cell * 4 / 10, rgb(255, 214, 10))
        circle(cx - cell / 10, cy - cell / 10, cell / 8, rgb(255, 240, 150))
      end
    end
  end
  for i in winning
    circle(ox + (i % COLS) * cell + cell / 2, oy + (i / COLS) * cell + cell / 2 + 4, cell / 7, "white")
  end
  if winner == 0 and turn == 2
    if think_at == 0
      think_at = ticks() + 300
    elif ticks() > think_at
      think_at = 0
      drop(computer_column())
    end
  end
  if winner != 0
    if button(W / 2 - 70, H - 50, 140, 40, "Play Again", true)
      restart()
    end
  end
end
