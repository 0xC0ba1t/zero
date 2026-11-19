# Bounce: a little physics toy. Click to throw a ball (drag to aim),
# right-click to clear, G toggles gravity.
window(640, 460, "Bounce")

balls = []
gravity = true
aim = []
colors = [rgb(255, 69, 58), rgb(255, 159, 10), rgb(255, 214, 10), rgb(48, 209, 88), rgb(100, 210, 255), rgb(10, 132, 255), rgb(191, 90, 242), rgb(255, 55, 95)]

func add_ball(x, y, vx, vy)
  push(balls, [x * 16, y * 16, vx, vy, 10 + rand(14), colors[rand(len(colors))]])
  if len(balls) > 40
    remove(balls, 0)
  end
end

for i = 0 to 5
  add_ball(100 + i * 80, 100, (rand(9) - 4) * 16, 0)
end

while frame()
  W = width()
  H = height()
  gradient(0, 0, W, H, rgb(30, 34, 52), rgb(16, 18, 28))
  rect(0, H - 6, W, 6, rgb(60, 66, 90))
  k = key()
  while k != 0
    if k == 'g'
      gravity = not gravity
    end
    k = key()
  end
  if rclicked()
    balls = []
  end
  if clicked()
    aim = [mousex(), mousey()]
  end
  if len(aim) == 2
    line(aim[0], aim[1], mousex(), mousey(), "white", 2)
    circle(aim[0], aim[1], 6, "white")
    if not mousedown()
      add_ball(aim[0], aim[1], (aim[0] - mousex()) * 3, (aim[1] - mousey()) * 3)
      sound("click")
      aim = []
    end
  end
  # move and bounce
  for b in balls
    if gravity
      b[3] = b[3] + 10
    end
    b[0] = b[0] + b[2]
    b[1] = b[1] + b[3]
    r = b[4] * 16
    if b[1] > (H - 6) * 16 - r
      b[1] = (H - 6) * 16 - r
      b[3] = -b[3] * 85 / 100
      b[2] = b[2] * 98 / 100
    end
    if b[1] < r
      b[1] = r
      b[3] = -b[3]
    end
    if b[0] < r
      b[0] = r
      b[2] = -b[2] * 9 / 10
    end
    if b[0] > W * 16 - r
      b[0] = W * 16 - r
      b[2] = -b[2] * 9 / 10
    end
  end
  # balls push each other apart
  n = len(balls)
  for i = 0 to n - 2
    for j = i + 1 to n - 1
      a = balls[i]
      c = balls[j]
      dxx = (c[0] - a[0]) / 16
      dyy = (c[1] - a[1]) / 16
      d = sqrt(dxx * dxx + dyy * dyy)
      m = a[4] + c[4]
      if d > 0 and d < m
        push_ = (m - d) * 8
        a[0] = a[0] - dxx * push_ / d
        a[1] = a[1] - dyy * push_ / d
        c[0] = c[0] + dxx * push_ / d
        c[1] = c[1] + dyy * push_ / d
        t = a[2]
        a[2] = c[2]
        c[2] = t
        t = a[3]
        a[3] = c[3]
        c[3] = t
      end
    end
  end
  for b in balls
    x = b[0] / 16
    y = b[1] / 16
    circle(x + 3, y + 5, b[4], rgb(8, 10, 16))
    circle(x, y, b[4], b[5])
    circle(x - b[4] / 3, y - b[4] / 3, b[4] / 3, mix(b[5], "white", 140))
  end
  g = "on"
  if not gravity
    g = "off"
  end
  text(14, 12, "click and drag to throw, right-click clears, G: gravity " + g, rgb(160, 170, 200), 16)
end
