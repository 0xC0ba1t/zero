# pong: you (left) against the computer (right), first to 5 wins
# keys: w / s or the arrow keys move your paddle, q quits

let top = 2
let bottom = 22

let i = 0
let k = 0

sub draw_field
  cls
  color 8
  i = 0
  while i < WIDTH
    plot i, 1, 205
    plot i, 23, 205
    i = i + 1
  end
  i = top
  while i <= bottom
    if i % 2 == 0
      plot 40, i, 250
    end
    i = i + 1
  end
  text 24, 24, "w/s or arrows: move   q: quit"
end

call draw_field
text 29, 12, "press any key to start"
k = getkey()
call draw_field
cls
