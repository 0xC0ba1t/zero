# Tic Tac Toe for the zero desktop. Click a square. The hard computer
# plays perfectly (minimax with alpha-beta pruning).
window(480, 560, "Tic Tac Toe")

LINES = [[0,1,2],[3,4,5],[6,7,8],[0,3,6],[1,4,7],[2,5,8],[0,4,8],[2,4,6]]
board = [0,0,0,0,0,0,0,0,0]
mode = 1
modes = ["Easy", "Hard", "2 Players"]
turn = 1
winner = 0
winline = []
score = [0, 0, 0]
think_at = 0
anim = [0,0,0,0,0,0,0,0,0]

func check(b)
  for l in LINES
    if b[l[0]] != 0 and b[l[0]] == b[l[1]] and b[l[1]] == b[l[2]]
      return b[l[0]]
    end
  end
  for v in b
    if v == 0
      return 0
    end
  end
  return 3
end

func line_of(b)
  for l in LINES
    if b[l[0]] != 0 and b[l[0]] == b[l[1]] and b[l[1]] == b[l[2]]
      return l
    end
  end
  return []
end

# score from O's (the computer's) point of view
func minimax(b, player, alpha, beta, depth)
  w = check(b)
  if w == 2
    return 10 - depth
  elif w == 1
    return depth - 10
  elif w == 3
    return 0
  end
  if player == 2
    best = -100
    for i = 0 to 8
      if b[i] == 0
        b[i] = 2
        v = minimax(b, 1, alpha, beta, depth + 1)
        b[i] = 0
        best = max(best, v)
        alpha = max(alpha, v)
        if beta <= alpha
          break
        end
      end
    end
    return best
  else
    best = 100
    for i = 0 to 8
      if b[i] == 0
        b[i] = 1
        v = minimax(b, 2, alpha, beta, depth + 1)
        b[i] = 0
        best = min(best, v)
        beta = min(beta, v)
        if beta <= alpha
          break
        end
      end
    end
    return best
  end
end

func computer_move()
  free = []
  for i = 0 to 8
    if board[i] == 0
      push(free, i)
    end
  end
  if mode == 0 and rand(100) < 60
    return free[rand(len(free))]
  end
  if len(free) == 9
    return 4
  end
  best = -1000
  pick = free[0]
  for i in free
    board[i] = 2
    v = minimax(board, 1, -1000, 1000, 0)
    board[i] = 0
    if v > best
      best = v
      pick = i
    end
  end
  return pick
end

func play(i)
  global turn, winner, winline
  board[i] = turn
  anim[i] = ticks()
  sound("click")
  winner = check(board)
  if winner != 0
    winline = line_of(board)
    score[winner - 1] = score[winner - 1] + 1
    if winner == 1
      emote(3)
    elif winner == 2 and mode != 2
      emote(5)
    else
      emote(1)
    end
    return
  end
  if turn == 1
    turn = 2
  else
    turn = 1
  end
end

func restart()
  global turn, winner, winline
  for i = 0 to 8
    board[i] = 0
  end
  turn = 1
  winner = 0
  winline = []
end

while frame()
  W = width()
  H = height()
  clear("bg")
  size = min(W - 60, H - 200)
  cell = size / 3
  ox = (W - cell * 3) / 2
  oy = 120
  # mode buttons
  for i = 0 to 2
    if button(W / 2 - 170 + i * 116, 16, 108, 34, modes[i], i == mode)
      mode = i
      restart()
    end
  end
  # status
  msg = ""
  if winner == 3
    msg = "It's a draw"
  elif winner == 1
    msg = "X wins!"
  elif winner == 2
    msg = "O wins!"
  elif turn == 1
    msg = "X to play"
  else
    msg = "O to play"
  end
  center(W / 2, 70, msg, "text", 24, true)
  # the grid
  rrect(ox - 8, oy - 8, cell * 3 + 16, cell * 3 + 16, 18, "panel")
  for i = 1 to 2
    rrect(ox + i * cell - 2, oy + 6, 4, cell * 3 - 12, 2, "line")
    rrect(ox + 6, oy + i * cell - 2, cell * 3 - 12, 4, 2, "line")
  end
  for i = 0 to 8
    cx = ox + (i % 3) * cell + cell / 2
    cy = oy + (i / 3) * cell + cell / 2
    age = min(ticks() - anim[i], 200)
    r = cell * 3 / 10 * age / 200
    if board[i] == 1
      line(cx - r, cy - r, cx + r, cy + r, rgb(10, 132, 255), 9)
      line(cx + r, cy - r, cx - r, cy + r, rgb(10, 132, 255), 9)
    elif board[i] == 2
      circle(cx, cy, r + 5, rgb(255, 69, 58))
      circle(cx, cy, max(r - 4, 0), "panel")
    elif winner == 0
      hover = mousex() > cx - cell / 2 and mousex() < cx + cell / 2 and mousey() > cy - cell / 2 and mousey() < cy + cell / 2
      if hover
        fade(cx - cell / 2 + 6, cy - cell / 2 + 6, cell - 12, cell - 12, "text", 20)
        if clicked() and (turn == 1 or mode == 2)
          play(i)
        end
      end
    end
  end
  if len(winline) == 3
    a = winline[0]
    b = winline[2]
    line(ox + (a % 3) * cell + cell / 2, oy + (a / 3) * cell + cell / 2, ox + (b % 3) * cell + cell / 2, oy + (b / 3) * cell + cell / 2, rgb(255, 214, 10), 7)
  end
  # the computer takes its turn after a short pause
  if winner == 0 and turn == 2 and mode != 2
    if think_at == 0
      think_at = ticks() + 350
    elif ticks() > think_at
      think_at = 0
      play(computer_move())
    end
  end
  text(ox, oy + cell * 3 + 22, "X  " + str(score[0]), rgb(10, 132, 255), 20, true)
  center(W / 2, oy + cell * 3 + 22, "draws " + str(score[2]), "muted", 20)
  textr(ox + cell * 3, oy + cell * 3 + 22, str(score[1]) + "  O", rgb(255, 69, 58), 20, true)
  if winner != 0
    if button(W / 2 - 70, H - 56, 140, 40, "Play Again", true)
      restart()
    end
  end
end
