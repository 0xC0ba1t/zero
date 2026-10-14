# a ball bouncing around the screen. press q to stop
cls
let x = 10
let y = 5
let dx = 1
let dy = 1
let running = 1
while running
  plot x, y, ' '
  x = x + dx
  y = y + dy
  if x <= 0 || x >= WIDTH - 1
    dx = -dx
  end
  if y <= 0 || y >= HEIGHT - 2
    dy = -dy
  end
  color 14
  plot x, y, 'O'
  sleep 30
  if key() == 'q'
    running = 0
  end
end
cls
