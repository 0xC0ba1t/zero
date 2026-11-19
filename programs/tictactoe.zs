# tic-tac-toe: you (X) against the computer (O)
# arrows or w/a/s/d move, enter or space places a mark, r restarts, q quits.
#   easy   - plays random squares (but grabs a win when it sees one)
#   normal - wins when it can, blocks you, likes the centre and the corners
#   hard   - perfect play (minimax with alpha-beta pruning): it cannot lose

dim b[9]
dim lines[24]
dim sd[12]
dim best[12]
dim m[12]
dim al[12]
dim be[12]

let level = 1
let cur = 4
let turn = 1
let winner = 0
let wline = 0
let running = 1
let choosing = 1
let k = 0
let i = 0
let j = 0
let w = 0
let moves = 0
let ply = 0
let p = 0
let ret = 0
let cx = 0
let cy = 0
let wins = 0
let losses = 0
let draws = 0
let pick = 0
let top = 0
let found = 0

# the eight winning lines, three cells each
sub init_lines
  lines[0] = 0 ; lines[1] = 1 ; lines[2] = 2
  lines[3] = 3 ; lines[4] = 4 ; lines[5] = 5
  lines[6] = 6 ; lines[7] = 7 ; lines[8] = 8
  lines[9] = 0 ; lines[10] = 3 ; lines[11] = 6
  lines[12] = 1 ; lines[13] = 4 ; lines[14] = 7
  lines[15] = 2 ; lines[16] = 5 ; lines[17] = 8
  lines[18] = 0 ; lines[19] = 4 ; lines[20] = 8
  lines[21] = 2 ; lines[22] = 4 ; lines[23] = 6
end

sub pick_level
  choosing = 1
  while choosing
    cls
    color 9
    text 33, 3, "T I C  T A C  T O E"
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

# sets winner: 0 = none yet, 1 = X, 2 = O, 3 = draw
sub check_win
  winner = 0
  i = 0
  while i < 8
    j = i * 3
    if b[lines[j]] != 0
      if b[lines[j]] == b[lines[j + 1]] && b[lines[j]] == b[lines[j + 2]]
        winner = b[lines[j]]
        wline = i
      end
    end
    i = i + 1
  end
  if winner == 0 && moves == 9
    winner = 3
  end
end

# a thick diagonal stroke made of small squares
sub stroke
  let t = 0
  while t < 56
    rect cx - 28 + t, cy - 28 + t, 7, 7
    rect cx + 28 - t - 6, cy - 28 + t, 7, 7
    t = t + 1
  end
end

sub draw_board
  cls
  color 15
  text 36, 1, "TIC TAC TOE"
  color 8
  rect 272, 40, 4, 288
  rect 368, 40, 4, 288
  rect 176, 132, 288, 4
  rect 176, 228, 288, 4
  i = 0
  while i < 9
    cx = 176 + (i % 3) * 96 + 48
    cy = 40 + (i / 3) * 96 + 48
    if b[i] == 1
      color 9
      call stroke
    end
    if b[i] == 2
      color 12
      disc cx, cy, 32
      color 0
      disc cx, cy, 23
    end
    i = i + 1
  end
  # the cursor
  if winner == 0 && turn == 1
    cx = 176 + (cur % 3) * 96
    cy = 40 + (cur / 3) * 96
    color 14
    rect cx + 4, cy + 4, 88, 3
    rect cx + 4, cy + 89, 88, 3
    rect cx + 4, cy + 4, 3, 88
    rect cx + 89, cy + 4, 3, 88
  end
  # the winning line
  if winner == 1 || winner == 2
    color 14
    i = wline * 3
    cx = 176 + (lines[i] % 3) * 96 + 48
    cy = 40 + (lines[i] / 3) * 96 + 48
    let ex = 176 + (lines[i + 2] % 3) * 96 + 48
    let ey = 40 + (lines[i + 2] / 3) * 96 + 48
    let s = 0
    while s <= 20
      disc cx + (ex - cx) * s / 20, cy + (ey - cy) * s / 20, 5
      s = s + 1
    end
  end
  color 7
  text 18, 21, "you ", wins, "   draws ", draws, "   cpu ", losses
  color 8
  text 11, 23, "arrows: move   enter: place   r: restart   q: quit"
  color 15
  if winner == 1
    text 34, 22, "YOU WIN!"
  end
  if winner == 2
    text 33, 22, "CPU WINS!"
  end
  if winner == 3
    text 35, 22, "A DRAW"
  end
end

# ---- the computer ----

# is there a square where `who` completes a line? result in found (or -1)
sub find_win
  found = -1
  i = 0
  while i < 9
    if b[i] == 0 && found < 0
      b[i] = pick
      moves = moves + 1
      call check_win
      if winner == pick
        found = i
      end
      b[i] = 0
      moves = moves - 1
      winner = 0
    end
    i = i + 1
  end
end

sub random_move
  found = rand(9)
  while b[found] != 0
    found = (found + 1) % 9
  end
end

# the minimax search; the position is in b, `ply` is the depth and sd[ply]
# the side to move. The score (for O) comes back in ret. Every local lives in
# an array indexed by ply, because variables are global.
sub solve
  p = ply
  winner = 0
  i = 0
  while i < 8
    j = i * 3
    if b[lines[j]] != 0
      if b[lines[j]] == b[lines[j + 1]] && b[lines[j]] == b[lines[j + 2]]
        winner = b[lines[j]]
      end
    end
    i = i + 1
  end
  if winner == 2
    ret = 10 - p
    return
  end
  if winner == 1
    ret = p - 10
    return
  end
  if moves == 9
    ret = 0
    return
  end
  if sd[p] == 2
    best[p] = -100
  else
    best[p] = 100
  end
  m[p] = 0
  while m[p] < 9
    if b[m[p]] == 0
      b[m[p]] = sd[p]
      moves = moves + 1
      sd[p + 1] = 3 - sd[p]
      al[p + 1] = al[p]
      be[p + 1] = be[p]
      ply = p + 1
      call solve
      ply = ply - 1
      p = ply
      moves = moves - 1
      b[m[p]] = 0
      if sd[p] == 2
        if ret > best[p]
          best[p] = ret
        end
        if best[p] > al[p]
          al[p] = best[p]
        end
      else
        if ret < best[p]
          best[p] = ret
        end
        if best[p] < be[p]
          be[p] = best[p]
        end
      end
      if al[p] >= be[p]
        m[p] = 9
      end
    end
    m[p] = m[p] + 1
  end
  ret = best[p]
end

sub hard_move
  # an empty board: just take the centre
  if moves == 0
    found = 4
    return
  end
  let bestscore = -100
  found = -1
  let c = 0
  while c < 9
    if b[c] == 0
      b[c] = 2
      moves = moves + 1
      sd[1] = 1
      al[1] = -100
      be[1] = 100
      ply = 1
      call solve
      moves = moves - 1
      b[c] = 0
      # a random choice between equally good moves keeps it from being predictable
      if ret > bestscore || (ret == bestscore && rand(3) == 0)
        bestscore = ret
        found = c
      end
    end
    c = c + 1
  end
end

sub normal_move
  pick = 2
  call find_win
  if found < 0
    pick = 1
    call find_win
  end
  if found < 0
    if b[4] == 0
      found = 4
    end
  end
  if found < 0
    # a free corner, otherwise anything
    let n = rand(4)
    let tries = 0
    while found < 0 && tries < 4
      let c = (n + tries) % 4
      let cell = 0
      if c == 1
        cell = 2
      end
      if c == 2
        cell = 6
      end
      if c == 3
        cell = 8
      end
      if b[cell] == 0
        found = cell
      end
      tries = tries + 1
    end
  end
  if found < 0
    call random_move
  end
end

sub easy_move
  pick = 2
  call find_win
  if found < 0 || rand(3) == 0
    call random_move
  end
end

sub cpu_turn
  sleep 450
  if level == 0
    call easy_move
  end
  if level == 1
    call normal_move
  end
  if level == 2
    call hard_move
  end
  b[found] = 2
  moves = moves + 1
  call check_win
  turn = 1
end

sub new_game
  i = 0
  while i < 9
    b[i] = 0
    i = i + 1
  end
  moves = 0
  winner = 0
  turn = 1
  cur = 4
end

sub finish_game
  if winner == 1
    wins = wins + 1
    emote 3
  end
  if winner == 2
    losses = losses + 1
    emote 5
  end
  if winner == 3
    draws = draws + 1
  end
end

call init_lines
call pick_level
call new_game
while running
  call draw_board
  if winner == 0 && turn == 2
    call cpu_turn
    if winner != 0
      call finish_game
    end
  else
    k = getkey()
    if k == 'q' || k == ESC
      running = 0
    end
    if k == 'r'
      call new_game
    end
    if winner == 0
      if (k == LEFT || k == 'a') && cur % 3 > 0
        cur = cur - 1
      end
      if (k == RIGHT || k == 'd') && cur % 3 < 2
        cur = cur + 1
      end
      if (k == UP || k == 'w') && cur > 2
        cur = cur - 3
      end
      if (k == DOWN || k == 's') && cur < 6
        cur = cur + 3
      end
      if (k == ENTER || k == SPACE) && b[cur] == 0
        b[cur] = 1
        moves = moves + 1
        call check_win
        turn = 2
        if winner != 0
          call finish_game
        end
      end
    else
      if k == ENTER || k == SPACE
        call new_game
      end
    end
  end
end
cls
color 15
print "Thanks for playing tic-tac-toe!"
