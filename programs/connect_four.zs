# connect four: you (red) against the computer (yellow)
# left/right (or a/d) pick a column, enter / space / down drops a disc,
# 1-7 drop straight into a column, r restarts, q quits.
#   easy   - random columns, but it takes a win when it sees one
#   normal - looks two moves ahead: it wins, blocks and avoids traps
#   hard   - looks five moves ahead with alpha-beta pruning

dim g[42]
dim colh[7]
dim wins[276]
dim hl[42]
dim order[7]
dim sd[12]
dim best[12]
dim mc[12]
dim cc[12]
dim al[12]
dim be[12]
dim dxs[4]
dim dys[4]

let level = 1
let maxd = 2
let col = 3
let turn = 1
let winner = 0
let running = 1
let choosing = 1
let k = 0
let i = 0
let j = 0
let n = 0
let pc = 0
let pr = 0
let pw = 0
let hit = 0
let ply = 0
let p = 0
let ret = 0
let found = 0
let score = 0
let cnt = 0
let x = 0
let y = 0
let d = 0
let a = 0
let b = 0
let cu = 0
let hu = 0
let em = 0
let ymy = 0
let w1 = 0
let w2 = 0
let pwins = 0
let cwins = 0
let anim = 0
let fall = 0
let bestscore = 0
let moves = 0

sub init_tables
  # column order for the search: centre first prunes best
  order[0] = 3 ; order[1] = 2 ; order[2] = 4 ; order[3] = 1
  order[4] = 5 ; order[5] = 0 ; order[6] = 6
  dxs[0] = 1 ; dys[0] = 0
  dxs[1] = 0 ; dys[1] = 1
  dxs[2] = 1 ; dys[2] = 1
  dxs[3] = 1 ; dys[3] = -1
  # every group of four cells that could win: 69 of them
  n = 0
  d = 0
  while d < 4
    y = 0
    while y < 6
      x = 0
      while x < 7
        let ex = x + dxs[d] * 3
        let ey = y + dys[d] * 3
        if ex >= 0 && ex < 7 && ey >= 0 && ey < 6
          i = 0
          while i < 4
            wins[n] = (y + dys[d] * i) * 7 + x + dxs[d] * i
            n = n + 1
            i = i + 1
          end
        end
        x = x + 1
      end
      y = y + 1
    end
    d = d + 1
  end
end

sub pick_level
  choosing = 1
  while choosing
    cls
    color 9
    text 31, 3, "C O N N E C T  F O U R"
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
  if level == 0
    maxd = 1
  end
  if level == 1
    maxd = 2
  end
  if level == 2
    maxd = 5
  end
end

# ---- the board ----

# drop a disc of player pw into column pc; the row it lands in comes back in pr
sub place
  pr = 5 - colh[pc]
  g[pr * 7 + pc] = pw
  colh[pc] = colh[pc] + 1
  moves = moves + 1
end

sub unplace
  colh[pc] = colh[pc] - 1
  g[(5 - colh[pc]) * 7 + pc] = 0
  moves = moves - 1
end

# hit = 1 if the disc at (pr, pc) belongs to a line of four; marks the line in hl
sub fourcheck
  hit = 0
  d = 0
  while d < 4
    cnt = 1
    x = pc + dxs[d]
    y = pr + dys[d]
    while x >= 0 && x < 7 && y >= 0 && y < 6 && g[y * 7 + x] == pw
      cnt = cnt + 1
      x = x + dxs[d]
      y = y + dys[d]
    end
    x = pc - dxs[d]
    y = pr - dys[d]
    while x >= 0 && x < 7 && y >= 0 && y < 6 && g[y * 7 + x] == pw
      cnt = cnt + 1
      x = x - dxs[d]
      y = y - dys[d]
    end
    if cnt >= 4
      hit = 1
      if anim
        # remember which discs formed the line
        hl[pr * 7 + pc] = 1
        x = pc + dxs[d]
        y = pr + dys[d]
        while x >= 0 && x < 7 && y >= 0 && y < 6 && g[y * 7 + x] == pw
          hl[y * 7 + x] = 1
          x = x + dxs[d]
          y = y + dys[d]
        end
        x = pc - dxs[d]
        y = pr - dys[d]
        while x >= 0 && x < 7 && y >= 0 && y < 6 && g[y * 7 + x] == pw
          hl[y * 7 + x] = 1
          x = x - dxs[d]
          y = y - dys[d]
        end
      end
    end
    d = d + 1
  end
end

# how good the position is for the computer (player 2)
sub evaluate
  score = 0
  # the centre column is worth the most
  i = 0
  while i < 6
    if g[i * 7 + 3] == 2
      score = score + 3
    end
    if g[i * 7 + 3] == 1
      score = score - 3
    end
    i = i + 1
  end
  a = 0
  while a < 69
    cu = 0
    hu = 0
    b = 0
    while b < 4
      let v = g[wins[a * 4 + b]]
      if v == 2
        cu = cu + 1
      end
      if v == 1
        hu = hu + 1
      end
      b = b + 1
    end
    if hu == 0
      if cu == 3
        score = score + 6
      end
      if cu == 2
        score = score + 2
      end
    end
    if cu == 0
      if hu == 3
        score = score - 7
      end
      if hu == 2
        score = score - 2
      end
    end
    a = a + 1
  end
  ret = score
end

# ---- drawing ----

sub draw_disc
  # a disc in slot (x, y) of colour `color`
  disc 176 + x * 48, 72 + y * 48, 19
end

sub draw_board
  cls
  color 15
  text 33, 0, "CONNECT FOUR"
  # the blue frame with holes
  color 1
  rect 144, 40, 352, 304
  y = 0
  while y < 6
    x = 0
    while x < 7
      color 0
      let v = g[y * 7 + x]
      if v == 1
        color 12
      end
      if v == 2
        color 14
      end
      call draw_disc
      if hl[y * 7 + x]
        color 15
        disc 176 + x * 48, 72 + y * 48, 21
        color 0
        disc 176 + x * 48, 72 + y * 48, 17
        if v == 1
          color 12
        else
          color 14
        end
        disc 176 + x * 48, 72 + y * 48, 12
      end
      x = x + 1
    end
    y = y + 1
  end
  color 7
  text 28, 23, "you ", pwins, "    cpu ", cwins
  color 8
  text 12, 24, "left/right: column   enter: drop   r: restart   q: quit"
end

sub draw_selector
  if turn == 1 && winner == 0
    color 12
    disc 176 + col * 48, 24, 11
    color 15
    text 35, 22, "your move"
  end
end

# a disc falling into column pc until row pr
sub animate_drop
  let bottom = 72 + pr * 48
  let yy = 24
  let speed = 5
  while yy < bottom
    yy = min(yy + speed, bottom)
    speed = speed + 2
    call draw_board
    if pw == 1
      color 12
    else
      color 14
    end
    disc 176 + pc * 48, yy, 19
    sleep 8
  end
end

sub show_result
  call draw_board
  color 15
  if winner == 1
    text 36, 22, "YOU WIN!"
  end
  if winner == 2
    text 35, 22, "CPU WINS!"
  end
  if winner == 3
    text 37, 22, "A DRAW"
  end
end

# ---- the computer ----

# minimax with alpha-beta pruning. ply is the depth, sd[ply] the player to move;
# the score (for player 2) comes back in ret. Locals live in arrays by ply.
sub solve
  p = ply
  if sd[p] == 2
    best[p] = -10000
  else
    best[p] = 10000
  end
  mc[p] = 0
  while mc[p] < 7
    cc[p] = order[mc[p]]
    if colh[cc[p]] < 6
      pc = cc[p]
      pw = sd[p]
      call place
      call fourcheck
      if hit
        if sd[p] == 2
          ret = 1000 - p
        else
          ret = p - 1000
        end
      else
        if p + 1 >= maxd || moves >= 42
          call evaluate
        else
          sd[p + 1] = 3 - sd[p]
          al[p + 1] = al[p]
          be[p + 1] = be[p]
          ply = p + 1
          call solve
          ply = ply - 1
          p = ply
        end
      end
      pc = cc[p]
      call unplace
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
        mc[p] = 7
      end
    end
    mc[p] = mc[p] + 1
  end
  if best[p] == 10000 || best[p] == -10000
    best[p] = 0
  end
  ret = best[p]
end

# a column where player pw would win right now (found = -1 if none)
sub winning_column
  found = -1
  i = 0
  while i < 7
    if colh[i] < 6 && found < 0
      pc = i
      call place
      anim = 0
      call fourcheck
      call unplace
      if hit
        found = i
      end
    end
    i = i + 1
  end
end

sub random_column
  found = rand(7)
  while colh[found] >= 6
    found = (found + 1) % 7
  end
end

sub choose_column
  pw = 2
  call winning_column
  if found >= 0
    return
  end
  if level == 0
    call random_column
    return
  end
  # search every column and keep the best (ties are broken at random)
  bestscore = -100000
  found = -1
  i = 0
  while i < 7
    j = order[i]
    if colh[j] < 6
      pc = j
      pw = 2
      call place
      anim = 0
      call fourcheck
      if hit
        ret = 1000
      else
        if moves >= 42 || maxd <= 1
          call evaluate
        else
          sd[1] = 1
          al[1] = -10000
          be[1] = 10000
          ply = 1
          call solve
        end
      end
      pc = j
      call unplace
      if ret > bestscore || (ret == bestscore && rand(3) == 0)
        bestscore = ret
        found = j
      end
    end
    i = i + 1
  end
end

sub cpu_turn
  call draw_board
  color 15
  text 35, 22, "thinking..."
  sleep 150
  call choose_column
  pc = found
  pw = 2
  call place
  anim = 1
  call fourcheck
  col = pc
  call animate_drop
  if hit
    winner = 2
    cwins = cwins + 1
    emote 5
  else
    if moves >= 42
      winner = 3
    end
  end
  turn = 1
end

sub human_drop
  if colh[col] < 6
    pc = col
    pw = 1
    call place
    anim = 1
    call fourcheck
    call animate_drop
    if hit
      winner = 1
      pwins = pwins + 1
      emote 3
    else
      if moves >= 42
        winner = 3
      end
    end
    turn = 2
  end
end

sub new_game
  i = 0
  while i < 42
    g[i] = 0
    hl[i] = 0
    i = i + 1
  end
  i = 0
  while i < 7
    colh[i] = 0
    i = i + 1
  end
  moves = 0
  winner = 0
  turn = 1
  col = 3
end

call init_tables
call pick_level
call new_game
while running
  call draw_board
  call draw_selector
  if winner != 0
    call show_result
  end
  if winner == 0 && turn == 2
    call cpu_turn
  else
    k = getkey()
    if k == 'q' || k == ESC
      running = 0
    end
    if k == 'r'
      call new_game
    end
    if winner == 0
      if (k == LEFT || k == 'a') && col > 0
        col = col - 1
      end
      if (k == RIGHT || k == 'd') && col < 6
        col = col + 1
      end
      if k >= '1' && k <= '7'
        col = k - '1'
        call human_drop
      end
      if k == ENTER || k == SPACE || k == DOWN || k == 's'
        call human_drop
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
print "Thanks for playing Connect Four!"
