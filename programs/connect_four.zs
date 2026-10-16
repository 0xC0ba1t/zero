# connect four: player (X) vs computer (O)
# controls: a/d to move column selector, ENTER/space to drop piece, q to quit

let col = 3
let running = 1
let turn = 1
let winner = 0
let k = 0
let i = 0

# Grid representation: 7 cols x 6 rows (42 cells total)
# 0 = empty, 1 = player X, 2 = computer O
let c0_0 = 0
let c1_0 = 0
let c2_0 = 0
let c3_0 = 0
let c4_0 = 0
let c5_0 = 0
let c6_0 = 0

let c0_1 = 0
let c1_1 = 0
let c2_1 = 0
let c3_1 = 0
let c4_1 = 0
let c5_1 = 0
let c6_1 = 0

let c0_2 = 0
let c1_2 = 0
let c2_2 = 0
let c3_2 = 0
let c4_2 = 0
let c5_2 = 0
let c6_2 = 0

let c0_3 = 0
let c1_3 = 0
let c2_3 = 0
let c3_3 = 0
let c4_3 = 0
let c5_3 = 0
let c6_3 = 0

sub draw_board
  cls
  color 15
  text 33, 2, "CONNECT FOUR"

  # Selector line
  color 14
  plot 26 + col * 4, 5, 'V'

  # Draw board frame
  color 9
  i = 0
  while i <= 6
    plot 24 + i * 4, 7, 209
    i = i + 1
  end

  color 7
  text 18, 22, "a/d: choose column   ENTER: drop   q: quit"
end

sub drop_piece
  # Drop piece into selected column `col`
  let dropped = 0
  if col == 0 && c0_0 == 0
    c0_0 = turn
    dropped = 1
  end
  if col == 1 && c1_0 == 0
    c1_0 = turn
    dropped = 1
  end
  if col == 2 && c2_0 == 0
    c2_0 = turn
    dropped = 1
  end
  if col == 3 && c3_0 == 0
    c3_0 = turn
    dropped = 1
  end
  if col == 4 && c4_0 == 0
    c4_0 = turn
    dropped = 1
  end
  if col == 5 && c5_0 == 0
    c5_0 = turn
    dropped = 1
  end
  if col == 6 && c6_0 == 0
    c6_0 = turn
    dropped = 1
  end

  if dropped
    if turn == 1
      turn = 2
    else
      turn = 1
    end
  end
end

sub computer_turn
  col = rand(7)
  call drop_piece
end

while running
  call draw_board

  if turn == 1
    k = getkey()
    if k == 'q' || k == ESC
      running = 0
    end
    if k == 'a' || k == 'A' || k == LEFT
      col = max(col - 1, 0)
    end
    if k == 'd' || k == 'D' || k == RIGHT
      col = min(col + 1, 6)
    end
    if k == ENTER || k == ' '
      call drop_piece
    end
  else
    sleep 400
    call computer_turn
  end
end

cls
color 15
print "Thanks for playing Connect Four!"