# Pong for the zero desktop. Your paddle follows the mouse, or W/S/arrows.
window(720, 480, "Pong")

goal = 7
level = 1
names = ["Easy", "Normal", "Hard"]
ai_speed = [5, 8, 12]
state = "menu"
wins = load("wins", 0)
py = 240
ay = 240
s1 = 0
s2 = 0
bx = 0
by = 0
vx = 0
vy = 0
serve_at = 0
trail = []

func reset_ball(towards)
  global bx, by, vx, vy, serve_at
  bx = width() / 2 * 16
  by = height() / 2 * 16
  vx = towards * 6 * 16
  vy = (rand(9) - 4) * 16
  serve_at = ticks() + 700
end

func new_game()
  global s1, s2, state, py, ay
  s1 = 0
  s2 = 0
  py = height() / 2
  ay = height() / 2
  state = "play"
  reset_ball(1)
end

while frame()
  W = width()
  H = height()
  PH = H / 5
  clear(rgb(18, 20, 26))
  for y = 10 to H - 10 step 28
    rrect(W / 2 - 2, y, 4, 16, 2, rgb(60, 64, 76))
  end
  center(W / 4, 24, str(s1), rgb(90, 96, 112), 32, true)
  center(W * 3 / 4, 24, str(s2), rgb(90, 96, 112), 32, true)
  if state == "play"
    if mousey() > 0 and mousex() > 0
      py = py + (mousey() - py) / 3
    end
    if keydown(UP) or keydown('w')
      py = py - 9
    end
    if keydown(DOWN) or keydown('s')
      py = py + 9
    end
    py = clamp(py, PH / 2, H - PH / 2)
    # the computer follows the ball, not perfectly
    target = by / 16
    if vx < 0
      target = H / 2
    end
    sp = ai_speed[level]
    if ay < target - 6
      ay = ay + sp
    elif ay > target + 6
      ay = ay - sp
    end
    ay = clamp(ay, PH / 2, H - PH / 2)
    if ticks() > serve_at
      bx = bx + vx
      by = by + vy
    end
    r = 9 * 16
    if by < r or by > H * 16 - r
      vy = -vy
      by = clamp(by, r, H * 16 - r)
      sound("click")
    end
    if vx < 0 and bx < 40 * 16 and bx > 18 * 16 and abs(by / 16 - py) < PH / 2 + 8
      vx = -vx * 106 / 100
      vy = (by / 16 - py) * 16 * 14 / PH
      sound("score")
    end
    if vx > 0 and bx > (W - 40) * 16 and bx < (W - 18) * 16 and abs(by / 16 - ay) < PH / 2 + 8
      vx = -vx * 106 / 100
      vy = (by / 16 - ay) * 16 * 14 / PH
      sound("score")
    end
    vx = clamp(vx, -22 * 16, 22 * 16)
    if bx < 0
      s2 = s2 + 1
      emote(2)
      reset_ball(1)
    elif bx > W * 16
      s1 = s1 + 1
      emote(1)
      reset_ball(-1)
    end
    if s1 >= goal or s2 >= goal
      state = "over"
      if s1 > s2
        wins = wins + 1
        save("wins", wins)
        emote(3)
      else
        emote(5)
      end
    end
    push(trail, [bx / 16, by / 16])
    if len(trail) > 8
      remove(trail, 0)
    end
    for i = 0 to len(trail) - 1
      t = trail[i]
      circle(t[0], t[1], 2 + i, mix(rgb(18, 20, 26), rgb(255, 214, 10), 30 * i))
    end
    circle(bx / 16, by / 16, 9, rgb(255, 214, 10))
  end
  rrect(24, py - PH / 2, 12, PH, 6, rgb(10, 132, 255))
  rrect(W - 36, ay - PH / 2, 12, PH, 6, rgb(255, 69, 58))
  if state != "play"
    fade(0, 0, W, H, "black", 140)
    if state == "menu"
      center(W / 2, H / 2 - 110, "PONG", "white", 32, true)
      center(W / 2, H / 2 - 70, "first to " + str(goal) + " wins. you have won " + str(wins) + " times", "white", 16)
    elif s1 > s2
      center(W / 2, H / 2 - 100, "You win!", rgb(48, 209, 88), 32, true)
    else
      center(W / 2, H / 2 - 100, "The computer wins", rgb(255, 69, 58), 32, true)
    end
    for i = 0 to 2
      if button(W / 2 - 165 + i * 112, H / 2 - 30, 100, 40, names[i], i == level)
        level = i
      end
    end
    if button(W / 2 - 80, H / 2 + 30, 160, 44, "Play", true)
      new_game()
    end
  end
end
