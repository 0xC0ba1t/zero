# Space Invaders for the zero desktop. Move with the mouse or the arrows,
# shoot with a click or Space. Clear a wave and a faster one comes.
window(640, 560, "Space Invaders")

ALIEN = ["..X.....X..", "...X...X...", "..XXXXXXX..", ".XX.XXX.XX.", "XXXXXXXXXXX", "X.XXXXXXX.X", "X.X.....X.X", "...XX.XX..."]
best = load("best", 0)
state = "menu"
ship = 320
aliens = []
shots = []
bombs = []
stars = []
for i = 0 to 60
  push(stars, [rand(640), rand(560), 1 + rand(3)])
end
dx = 1
wave = 1
lives = 3
score = 0
last_move = 0
last_shot = 0
boom = []

func new_wave()
  global aliens, dx, last_move
  aliens = []
  for r = 0 to 4
    for c = 0 to 9
      push(aliens, [40 + c * 46, 70 + r * 38, r])
    end
  end
  dx = 1
  last_move = ticks()
end

func new_game()
  global score, lives, wave, state, shots, bombs
  score = 0
  lives = 3
  wave = 1
  shots = []
  bombs = []
  state = "play"
  new_wave()
end

func draw_alien(x, y, kind, frame2)
  colors = [rgb(255, 99, 132), rgb(255, 159, 64), rgb(255, 205, 86), rgb(75, 192, 192), rgb(54, 162, 235)]
  px = 3
  for ry = 0 to 7
    row = ALIEN[ry]
    for rx = 0 to 10
      if sub(row, rx, 1) == "X"
        yy = ry
        if frame2 == 1 and ry == 7
          yy = 6
        end
        rect(x + rx * px, y + yy * px, px, px, colors[kind])
      end
    end
  end
end

while frame()
  W = width()
  H = height()
  clear(rgb(6, 8, 20))
  for s in stars
    s[1] = s[1] + s[2]
    if s[1] > H
      s[1] = 0
      s[0] = rand(W)
    end
    rect(s[0], s[1], s[2], s[2], rgb(120 + s[2] * 40, 120 + s[2] * 40, 180))
  end
  text(16, 12, "Score " + str(score), "white", 20, true)
  center(W / 2, 12, "Wave " + str(wave), rgb(140, 150, 180), 20)
  textr(W - 16, 12, "Best " + str(best), rgb(140, 150, 180), 20)
  for i = 1 to lives
    rrect(W - 30 * i - 10, H - 24, 22, 10, 3, rgb(48, 209, 88))
  end
  if state == "play"
    # move the ship
    if mousex() > 0
      ship = ship + (mousex() - ship) / 3
    end
    if keydown(LEFT) or keydown('a')
      ship = ship - 7
    end
    if keydown(RIGHT) or keydown('d')
      ship = ship + 7
    end
    ship = clamp(ship, 24, W - 24)
    k = key()
    fire = clicked() or keydown(SPACE)
    while k != 0
      if k == SPACE
        fire = true
      end
      k = key()
    end
    if fire and ticks() - last_shot > 280
      push(shots, [ship, H - 70])
      last_shot = ticks()
      sound("click")
    end
    # the swarm marches
    period = max(120, 650 - len(aliens) * 9 - wave * 40)
    if ticks() - last_move > period
      last_move = ticks()
      edge = false
      for a in aliens
        a[0] = a[0] + dx * 12
        if a[0] < 10 or a[0] > W - 45
          edge = true
        end
      end
      if edge
        dx = -dx
        for a in aliens
          a[1] = a[1] + 18
          if a[1] > H - 110
            state = "over"
          end
        end
      end
      if len(aliens) > 0 and rand(100) < 30 + wave * 5
        a = aliens[rand(len(aliens))]
        push(bombs, [a[0] + 16, a[1] + 24])
      end
    end
    # shots
    i = 0
    while i < len(shots)
      s = shots[i]
      s[1] = s[1] - 9
      hit = false
      j = 0
      while j < len(aliens)
        a = aliens[j]
        if s[0] >= a[0] and s[0] <= a[0] + 33 and s[1] >= a[1] and s[1] <= a[1] + 24
          push(boom, [a[0] + 16, a[1] + 12, ticks()])
          score = score + (5 - a[2]) * 10
          remove(aliens, j)
          hit = true
          sound("score")
          break
        end
        j = j + 1
      end
      if hit or s[1] < 30
        remove(shots, i)
      else
        i = i + 1
      end
    end
    # bombs
    i = 0
    while i < len(bombs)
      b = bombs[i]
      b[1] = b[1] + 5 + wave
      if b[1] > H - 68 and b[1] < H - 44 and abs(b[0] - ship) < 22
        lives = lives - 1
        emote(2)
        push(boom, [ship, H - 56, ticks()])
        remove(bombs, i)
        if lives <= 0
          state = "over"
        end
      elif b[1] > H
        remove(bombs, i)
      else
        i = i + 1
      end
    end
    if len(aliens) == 0
      wave = wave + 1
      emote(3)
      say("wave " + str(wave) + "!")
      new_wave()
    end
    if state == "over"
      emote(5)
      if score > best
        best = score
        save("best", best)
      end
    end
  end
  # draw everything
  fr = (ticks() / 400) % 2
  for a in aliens
    draw_alien(a[0], a[1], a[2], fr)
  end
  for s in shots
    rrect(s[0] - 2, s[1], 4, 14, 2, rgb(120, 255, 160))
  end
  for b in bombs
    rrect(b[0] - 3, b[1], 6, 12, 3, rgb(255, 120, 90))
  end
  i = 0
  while i < len(boom)
    e = boom[i]
    age = ticks() - e[2]
    if age > 400
      remove(boom, i)
    else
      circle(e[0], e[1], 4 + age / 20, mix(rgb(255, 214, 10), rgb(6, 8, 20), age * 255 / 400))
      i = i + 1
    end
  end
  # the ship
  sy = H - 64
  rrect(ship - 22, sy + 8, 44, 14, 5, rgb(48, 209, 88))
  rrect(ship - 5, sy - 4, 10, 16, 3, rgb(48, 209, 88))
  if state != "play"
    fade(0, 0, W, H, "black", 150)
    if state == "menu"
      center(W / 2, H / 2 - 110, "SPACE INVADERS", "white", 32, true)
      center(W / 2, H / 2 - 66, "mouse or arrows move, click or space shoots", rgb(180, 190, 210), 16)
    else
      center(W / 2, H / 2 - 110, "Game over", "white", 32, true)
      center(W / 2, H / 2 - 66, "score " + str(score) + "   best " + str(best), rgb(180, 190, 210), 16)
    end
    if button(W / 2 - 80, H / 2 - 10, 160, 44, "Play", true)
      new_game()
    end
  end
end
