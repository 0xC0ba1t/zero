# snake: classic arcade game
# controls: w/a/s/d or up/down arrows to turn, q to quit

let head_x = 20
let head_y = 12
let dir_x = 1
let dir_y = 0

# snake body segment arrays (up to 10 segments)
let s0_x = 20
let s0_y = 12
let s1_x = 19
let s1_y = 12
let s2_x = 18
let s2_y = 12
let s3_x = 0
let s3_y = 0
let s4_x = 0
let s4_y = 0
let s5_x = 0
let s5_y = 0
let s6_x = 0
let s6_y = 0
let s7_x = 0
let s7_y = 0
let s8_x = 0
let s8_y = 0
let s9_x = 0
let s9_y = 0

let length = 3
let food_x = 35
let food_y = 12
let score = 0
let running = 1
let k = 0
let i = 0
let tail_x = 0
let tail_y = 0

sub spawn_food
  food_x = 3 + rand(72)
  food_y = 3 + rand(18)
end

sub draw_borders
  cls
  color 8
  i = 0
  while i < WIDTH
    plot i, 1, 205
    plot i, 23, 205
    i = i + 1
  end
  i = 1
  while i <= 23
    plot 0, i, 186
    plot 79, i, 186
    i = i + 1
  end
  color 15
  text 32, 0, "S N A K E"
  text 2, 0, "Score: ", score
  text 24, 24, "w/a/s/d or up/down: turn   q: quit"
end

sub shift_body
  # erase old tail position
  if length == 3
    tail_x = s2_x
    tail_y = s2_y
  end
  if length == 4
    tail_x = s3_x
    tail_y = s3_y
  end
  if length == 5
    tail_x = s4_x
    tail_y = s4_y
  end
  if length == 6
    tail_x = s5_x
    tail_y = s5_y
  end
  if length == 7
    tail_x = s6_x
    tail_y = s6_y
  end
  if length == 8
    tail_x = s7_x
    tail_y = s7_y
  end
  if length == 9
    tail_x = s8_x
    tail_y = s8_y
  end
  if length >= 10
    tail_x = s9_x
    tail_y = s9_y
  end

  plot tail_x, tail_y, ' '

  s9_x = s8_x
  s9_y = s8_y
  s8_x = s7_x
  s8_y = s7_y
  s7_x = s6_x
  s7_y = s6_y
  s6_x = s5_x
  s6_y = s5_y
  s5_x = s4_x
  s5_y = s4_y
  s4_x = s3_x
  s4_y = s3_y
  s3_x = s2_x
  s3_y = s2_y
  s2_x = s1_x
  s2_y = s1_y
  s1_x = s0_x
  s1_y = s0_y

  s0_x = head_x
  s0_y = head_y
end

sub draw_snake
  color 10
  plot s0_x, s0_y, 'O'
  if length > 1
    plot s1_x, s1_y, 'o'
  end
  if length > 2
    plot s2_x, s2_y, 'o'
  end
  if length > 3
    plot s3_x, s3_y, 'o'
  end
  if length > 4
    plot s4_x, s4_y, 'o'
  end
  if length > 5
    plot s5_x, s5_y, 'o'
  end
  if length > 6
    plot s6_x, s6_y, 'o'
  end
  if length > 7
    plot s7_x, s7_y, 'o'
  end
  if length > 8
    plot s8_x, s8_y, 'o'
  end
  if length > 9
    plot s9_x, s9_y, 'o'
  end
end

sub check_self_collision
  if length > 2 && head_x == s2_x && head_y == s2_y
    running = 0
  end
  if length > 3 && head_x == s3_x && head_y == s3_y
    running = 0
  end
  if length > 4 && head_x == s4_x && head_y == s4_y
    running = 0
  end
  if length > 5 && head_x == s5_x && head_y == s5_y
    running = 0
  end
  if length > 6 && head_x == s6_x && head_y == s6_y
    running = 0
  end
  if length > 7 && head_x == s7_x && head_y == s7_y
    running = 0
  end
  if length > 8 && head_x == s8_x && head_y == s8_y
    running = 0
  end
  if length > 9 && head_x == s9_x && head_y == s9_y
    running = 0
  end
end

call draw_borders
call spawn_food

while running
  # Food display
  color 12
  plot food_x, food_y, '*'

  k = key()
  if k == 'q' || k == ESC
    running = 0
  end
  if (k == 'w' || k == 'W' || k == UP) && dir_y != 1
    dir_x = 0
    dir_y = -1
  end
  if (k == 's' || k == 'S' || k == DOWN) && dir_y != -1
    dir_x = 0
    dir_y = 1
  end
  if (k == 'a' || k == 'A') && dir_x != 1
    dir_x = -1
    dir_y = 0
  end
  if (k == 'd' || k == 'D') && dir_x != -1
    dir_x = 1
    dir_y = 0
  end

  head_x = head_x + dir_x
  head_y = head_y + dir_y

  # Check wall collision
  if head_x <= 0 || head_x >= 79 || head_y <= 1 || head_y >= 23
    running = 0
  end

  call check_self_collision

  # Check food collision
  if head_x == food_x && head_y == food_y
    score = score + 10
    length = min(length + 1, 10)
    color 15
    text 2, 0, "Score: ", score
    call spawn_food
  end

  if running
    call shift_body
    call draw_snake
  end

  sleep 80
end

cls
color 15
print "Game Over! Your final score was: ", score