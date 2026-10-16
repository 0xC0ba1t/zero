# tic tac toe: player (X) against the computer (O)
# keys: w/s/a/d or up/down to move, ENTER or space to mark, q to quit

let m0 = 0
let m1 = 0
let m2 = 0
let m3 = 0
let m4 = 0
let m5 = 0
let m6 = 0
let m7 = 0
let m8 = 0

let cx = 1
let cy = 1
let bx = 0
let by = 0
let val = 0
let cell_idx = 0
let running = 1
let quit = 0
let winner = 0
let k = 0
let i = 0
let turn = 1

sub draw_cell
  if cell_idx == 0
    bx = 33
    by = 9
    val = m0
  end
  if cell_idx == 1
    bx = 41
    by = 9
    val = m1
  end
  if cell_idx == 2
    bx = 49
    by = 9
    val = m2
  end
  if cell_idx == 3
    bx = 33
    by = 13
    val = m3
  end
  if cell_idx == 4
    bx = 41
    by = 13
    val = m4
  end
  if cell_idx == 5
    bx = 49
    by = 13
    val = m5
  end
  if cell_idx == 6
    bx = 33
    by = 17
    val = m6
  end
  if cell_idx == 7
    bx = 41
    by = 17
    val = m7
  end
  if cell_idx == 8
    bx = 49
    by = 17
    val = m8
  end

  if val == 1
    color 11
    plot bx, by, 'X'
  end
  if val == 2
    color 12
    plot bx, by, 'O'
  end
  if val == 0
    plot bx, by, ' '
  end
end

sub draw_marks
  cell_idx = 0
  while cell_idx < 9
    call draw_cell
    cell_idx = cell_idx + 1
  end
end

sub draw_cursor
  if cy == 0
    by = 9
  end
  if cy == 1
    by = 13
  end
  if cy == 2
    by = 17
  end

  if cx == 0
    bx = 33
  end
  if cx == 1
    bx = 41
  end
  if cx == 2
    bx = 49
  end

  color 14
  plot bx - 2, by, '['
  plot bx + 2, by, ']'
end

sub draw_grid
  cls
  color 15
  text 34, 2, "TIC TAC TOE"
  color 8

  # vertical grid lines
  i = 7
  while i <= 19
    plot 37, i, 179
    plot 45, i, 179
    i = i + 1
  end

  # horizontal grid lines
  i = 30
  while i <= 52
    plot i, 11, 196
    plot i, 15, 196
    i = i + 1
  end

  color 7
  text 16, 22, "w/s/a/d or up/down: move   ENTER: mark   q: quit"
  call draw_marks
  call draw_cursor
end

sub check_win
  winner = 0

  # Rows
  if m0 != 0 && m0 == m1 && m1 == m2
    winner = m0
  end
  if m3 != 0 && m3 == m4 && m4 == m5
    winner = m3
  end
  if m6 != 0 && m6 == m7 && m7 == m8
    winner = m6
  end

  # Cols
  if m0 != 0 && m0 == m3 && m3 == m6
    winner = m0
  end
  if m1 != 0 && m1 == m4 && m4 == m7
    winner = m1
  end
  if m2 != 0 && m2 == m5 && m5 == m8
    winner = m2
  end

  # Diagonals
  if m0 != 0 && m0 == m4 && m4 == m8
    winner = m0
  end
  if m2 != 0 && m2 == m4 && m4 == m6
    winner = m2
  end

  # Tie check
  if winner == 0
    if m0 != 0 && m1 != 0 && m2 != 0 && m3 != 0 && m4 != 0 && m5 != 0 && m6 != 0 && m7 != 0 && m8 != 0
      winner = 3
    end
  end
end

sub player_place
  if cx == 0 && cy == 0
    if m0 == 0
      m0 = 1
      turn = 2
    end
  end
  if cx == 1 && cy == 0
    if m1 == 0
      m1 = 1
      turn = 2
    end
  end
  if cx == 2 && cy == 0
    if m2 == 0
      m2 = 1
      turn = 2
    end
  end
  if cx == 0 && cy == 1
    if m3 == 0
      m3 = 1
      turn = 2
    end
  end
  if cx == 1 && cy == 1
    if m4 == 0
      m4 = 1
      turn = 2
    end
  end
  if cx == 2 && cy == 1
    if m5 == 0
      m5 = 1
      turn = 2
    end
  end
  if cx == 0 && cy == 2
    if m6 == 0
      m6 = 1
      turn = 2
    end
  end
  if cx == 1 && cy == 2
    if m7 == 0
      m7 = 1
      turn = 2
    end
  end
  if cx == 2 && cy == 2
    if m8 == 0
      m8 = 1
      turn = 2
    end
  end
end

sub computer_move
  let moved = 0
  if m4 == 0
    m4 = 2
    moved = 1
  end
  if moved == 0 && m0 == 0
    m0 = 2
    moved = 1
  end
  if moved == 0 && m2 == 0
    m2 = 2
    moved = 1
  end
  if moved == 0 && m6 == 0
    m6 = 2
    moved = 1
  end
  if moved == 0 && m8 == 0
    m8 = 2
    moved = 1
  end
  if moved == 0 && m1 == 0
    m1 = 2
    moved = 1
  end
  if moved == 0 && m3 == 0
    m3 = 2
    moved = 1
  end
  if moved == 0 && m5 == 0
    m5 = 2
    moved = 1
  end
  if moved == 0 && m7 == 0
    m7 = 2
    moved = 1
  end
  turn = 1
end

while running
  call draw_grid

  if turn == 1
    k = getkey()
    if k == 'q' || k == ESC
      running = 0
      quit = 1
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
    cx = min(cx, 2)
    cy = max(cy, 0)
    cy = min(cy, 2)

    if k == ENTER || k == ' '
      call player_place
      call check_win
      if winner > 0
        running = 0
      end
    end
  else
    sleep 400
    call computer_move
    call check_win
    if winner > 0
      running = 0
    end
  end

  sleep 50
end

cls
color 15
if quit
  print "bye! game exited."
else
  if winner == 1
    print "congratulations, you win!"
  end
  if winner == 2
    print "the computer wins!"
  end
  if winner == 3
    print "it's a draw!"
  end
end