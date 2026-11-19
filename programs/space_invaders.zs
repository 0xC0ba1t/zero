# space invaders
# a/d or left/right move (hold them), space shoots (hold for auto-fire),
# p pauses, q quits.
#   easy   - 5 lives, slow shots, a slow formation
#   normal - 3 lives
#   hard   - 2 lives, fast and frequent enemy fire, a quick formation
# Positions are in pixels of a 640x400 screen.

dim alive[50]
dim sh[264]
dim ebx[4]
dim eby[4]
dim eon[4]
dim fxs[8]
dim fys[8]
dim fts[8]

let level = 1
let running = 1
let choosing = 1
let k = 0
let i = 0
let j = 0
let r = 0
let c = 0
let t0 = 0

# game state
let lives = 3
let score = 0
let best = 0
let wave = 1
let over = 0
let px = 306
let bx = 0
let by = 0
let bon = 0
let shot_at = 0
let left = 0
let fx = 60
let fy = 48
let fdir = 1
let fframe = 0
let tnext = 0
let base_step = 520
let fire_odds = 80
let ebullet_speed = 4
let ufo_x = 0
let ufo_on = 0
let ufo_dir = 1
let ufo_next = 0
let pts_x = 0
let pts_n = 0
let pts_t = 0
let hit_t = 0
let sx = 0
let sy = 0
let col = 0
let row = 0
let gx = 0
let gy = 0
let n = 0
let lowest = 0
let minx = 0
let maxx = 0
let kind = 0
let ex = 0
let ey = 0
let paused = 0
let spawn = 0
let fe = 0

sub pick_level
  choosing = 1
  while choosing
    cls
    color 9
    text 27, 3, "S P A C E  I N V A D E R S"
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

# ---- drawing ----

sub draw_hud
  color 0
  rect 0, 0, 640, 18
  color 15
  text 2, 0, "SCORE ", score
  text 32, 0, "WAVE ", wave
  color 12
  text 62, 0, "LIVES ", lives
end

# the invader of kind `kind` (0 squid, 1 crab, 2 octopus) with its top left at (sx, sy)
sub draw_invader
  if kind == 0
    color 13
    rect sx + 7, sy, 10, 3
    rect sx + 5, sy + 3, 14, 7
    color 0
    rect sx + 8, sy + 5, 3, 3
    rect sx + 13, sy + 5, 3, 3
    color 13
    if fframe
      rect sx + 3, sy + 10, 3, 5
      rect sx + 18, sy + 10, 3, 5
      rect sx + 9, sy + 10, 2, 3
      rect sx + 13, sy + 10, 2, 3
    else
      rect sx + 5, sy + 10, 3, 5
      rect sx + 16, sy + 10, 3, 5
      rect sx + 7, sy + 10, 2, 3
      rect sx + 15, sy + 10, 2, 3
    end
  end
  if kind == 1
    color 11
    rect sx + 4, sy, 3, 3
    rect sx + 17, sy, 3, 3
    rect sx + 2, sy + 3, 20, 7
    color 0
    rect sx + 7, sy + 5, 3, 3
    rect sx + 14, sy + 5, 3, 3
    color 11
    if fframe
      rect sx, sy + 5, 3, 8
      rect sx + 21, sy + 5, 3, 8
      rect sx + 5, sy + 10, 3, 4
      rect sx + 16, sy + 10, 3, 4
    else
      rect sx + 1, sy + 3, 3, 6
      rect sx + 20, sy + 3, 3, 6
      rect sx + 3, sy + 10, 3, 4
      rect sx + 18, sy + 10, 3, 4
    end
  end
  if kind == 2
    color 10
    rect sx + 5, sy, 14, 3
    rect sx + 2, sy + 3, 20, 7
    color 0
    rect sx + 7, sy + 5, 3, 3
    rect sx + 14, sy + 5, 3, 3
    color 10
    if fframe
      rect sx + 3, sy + 10, 4, 3
      rect sx + 17, sy + 10, 4, 3
      rect sx + 8, sy + 10, 8, 3
    else
      rect sx + 1, sy + 10, 4, 3
      rect sx + 19, sy + 10, 4, 3
      rect sx + 6, sy + 10, 12, 3
    end
  end
end

sub draw_formation
  # wipe the whole formation area, then draw every invader that is alive
  color 0
  rect 0, 20, 640, 270
  r = 0
  while r < 5
    c = 0
    while c < 10
      if alive[r * 10 + c]
        sx = fx + c * 36
        sy = fy + r * 28
        kind = 2
        if r == 0
          kind = 0
        end
        if r == 1 || r == 2
          kind = 1
        end
        call draw_invader
      end
      c = c + 1
    end
    r = r + 1
  end
end

sub draw_ship
  color 10
  rect px, 358, 28, 8
  rect px + 10, 352, 8, 6
  rect px + 13, 347, 2, 5
end

sub erase_ship
  color 0
  rect px - 2, 345, 32, 22
end

sub draw_shields
  i = 0
  while i < 4
    j = 0
    while j < 66
      if sh[i * 66 + j]
        color 2
        rect 70 + i * 140 + (j % 11) * 5, 300 + (j / 11) * 5, 5, 5
      end
      j = j + 1
    end
    i = i + 1
  end
end

sub build_shields
  i = 0
  while i < 4
    j = 0
    while j < 66
      r = j / 11
      c = j % 11
      sh[i * 66 + j] = 1
      if r == 0 && (c < 2 || c > 8)
        sh[i * 66 + j] = 0
      end
      if r >= 4 && c >= 3 && c <= 7
        sh[i * 66 + j] = 0
      end
      j = j + 1
    end
    i = i + 1
  end
end

# ---- setup ----

sub start_wave
  i = 0
  while i < 50
    alive[i] = 1
    i = i + 1
  end
  fx = 60
  fy = 44 + min((wave - 1) * 10, 50)
  fdir = 1
  fframe = 0
  bon = 0
  i = 0
  while i < 4
    eon[i] = 0
    i = i + 1
  end
  left = 50
  ufo_on = 0
  ufo_next = ticks() + 8000 + rand(8000)
  tnext = ticks() + 600
  color 0
  rect 0, 20, 640, 330
  call build_shields
  call draw_shields
  call draw_formation
  call draw_ship
  call draw_hud
end

sub setup_level
  lives = 3
  base_step = 520
  fire_odds = 80
  ebullet_speed = 4
  if level == 0
    lives = 5
    base_step = 700
    fire_odds = 150
    ebullet_speed = 3
  end
  if level == 2
    lives = 2
    base_step = 380
    fire_odds = 45
    ebullet_speed = 5
  end
end

# ---- the formation moves ----

sub step_formation
  # the extent of what is left
  minx = 9999
  maxx = -9999
  lowest = 0
  r = 0
  while r < 5
    c = 0
    while c < 10
      if alive[r * 10 + c]
        minx = min(minx, fx + c * 36)
        maxx = max(maxx, fx + c * 36 + 24)
        lowest = max(lowest, fy + r * 28 + 16)
      end
      c = c + 1
    end
    r = r + 1
  end
  fframe = 1 - fframe
  if (fdir > 0 && maxx + 8 > 628) || (fdir < 0 && minx - 8 < 12)
    fdir = -fdir
    fy = fy + 14
  else
    fx = fx + fdir * 8
  end
  call draw_formation
  if lowest + 14 >= 340
    over = 1
    emote 5
  end
end

# ---- shooting ----

# a shot at (gx, gy): did it hit a bunker block? result in hit_t (1 = yes)
sub hit_shield
  hit_t = 0
  if gy >= 300 && gy < 330
    i = (gx - 70) / 140
    if gx >= 70 && i >= 0 && i < 4
      c = (gx - 70 - i * 140) / 5
      r = (gy - 300) / 5
      if c >= 0 && c < 11 && r >= 0 && r < 6 && (gx - 70 - i * 140) < 55
        if sh[i * 66 + r * 11 + c]
          sh[i * 66 + r * 11 + c] = 0
          color 0
          rect 70 + i * 140 + c * 5, 300 + r * 5, 5, 5
          # a bite out of the neighbours too
          if c + 1 < 11 && sh[i * 66 + r * 11 + c + 1] && rand(2) == 0
            sh[i * 66 + r * 11 + c + 1] = 0
            rect 70 + i * 140 + (c + 1) * 5, 300 + r * 5, 5, 5
          end
          if r + 1 < 6 && sh[i * 66 + (r + 1) * 11 + c] && rand(2) == 0
            sh[i * 66 + (r + 1) * 11 + c] = 0
            rect 70 + i * 140 + c * 5, 300 + (r + 1) * 5, 5, 5
          end
          hit_t = 1
        end
      end
    end
  end
end

sub flash
  color 15
  disc ex, ey, 9
  sleep 25
  color 0
  disc ex, ey, 10
end

sub player_shot
  if bon
    # erase, move, test
    color 0
    rect bx, by, 2, 9
    by = by - 9
    if by < 22
      bon = 0
      return
    end
    # a bunker?
    gx = bx
    gy = by
    call hit_shield
    if hit_t
      bon = 0
      return
    end
    # an invader?
    col = (bx - fx) / 36
    row = (by - fy) / 28
    if bx >= fx && by >= fy && col >= 0 && col < 10 && row >= 0 && row < 5
      if alive[row * 10 + col] && (bx - fx) % 36 < 24 && (by - fy) % 28 < 16
        alive[row * 10 + col] = 0
        left = left - 1
        sx = fx + col * 36
        sy = fy + row * 28
        color 0
        rect sx - 1, sy - 1, 26, 18
        ex = sx + 12
        ey = sy + 8
        call flash
        n = 10
        if row == 0
          n = 30
        end
        if row == 1 || row == 2
          n = 20
        end
        score = score + n
        best = max(best, score)
        emote 1
        call draw_hud
        bon = 0
        return
      end
    end
    # the saucer?
    if ufo_on && by < 46 && bx >= ufo_x && bx < ufo_x + 30
      color 0
      rect ufo_x - 1, 28, 34, 14
      n = (1 + rand(3)) * 100
      score = score + n
      best = max(best, score)
      pts_x = ufo_x
      pts_n = n
      pts_t = ticks() + 900
      ufo_on = 0
      bon = 0
      call draw_hud
      return
    end
    color 15
    rect bx, by, 2, 9
  end
end

sub enemy_fire
  # a random column fires from its lowest invader
  if rand(fire_odds) == 0
    i = 0
    while i < 4
      if eon[i] == 0
        c = rand(10)
        r = 4
        while r >= 0 && alive[r * 10 + c] == 0
          r = r - 1
        end
        if r >= 0
          ebx[i] = fx + c * 36 + 11
          eby[i] = fy + r * 28 + 16
          eon[i] = 1
        end
        i = 4
      end
      i = i + 1
    end
  end
end

sub enemy_shots
  i = 0
  while i < 4
    if eon[i]
      color 0
      rect ebx[i], eby[i], 4, 9
      eby[i] = eby[i] + ebullet_speed
      if eby[i] > 370
        eon[i] = 0
      else
        gx = ebx[i] + 1
        gy = eby[i] + 9
        call hit_shield
        if hit_t
          eon[i] = 0
        else
          # the ship?
          if eby[i] + 9 >= 352 && eby[i] <= 366 && ebx[i] + 4 >= px && ebx[i] <= px + 28
            eon[i] = 0
            lives = lives - 1
            emote 2
            call erase_ship
            ex = px + 14
            ey = 358
            color 12
            disc ex, ey, 14
            sleep 250
            color 0
            disc ex, ey, 15
            sleep 350
            if lives <= 0
              over = 1
              emote 5
            end
            call draw_ship
            call draw_hud
          else
            color 12
            rect ebx[i], eby[i], 4, 9
          end
        end
      end
    end
    i = i + 1
  end
end

sub saucer
  if ufo_on == 0 && ticks() > ufo_next
    ufo_on = 1
    ufo_dir = 1 - rand(2) * 2
    ufo_x = 4
    if ufo_dir < 0
      ufo_x = 604
    end
  end
  if ufo_on
    color 0
    rect ufo_x - 1, 28, 34, 14
    ufo_x = ufo_x + ufo_dir * 3
    if ufo_x < 0 || ufo_x > 610
      ufo_on = 0
      ufo_next = ticks() + 9000 + rand(9000)
    else
      color 12
      rect ufo_x + 6, 28, 18, 4
      rect ufo_x + 2, 32, 26, 5
      rect ufo_x + 8, 37, 4, 2
      rect ufo_x + 18, 37, 4, 2
      color 14
      rect ufo_x + 8, 33, 3, 2
      rect ufo_x + 14, 33, 3, 2
      rect ufo_x + 20, 33, 3, 2
    end
  end
  if pts_n > 0 && ticks() > pts_t
    color 0
    rect pts_x - 2, 28, 50, 16
    pts_n = 0
  end
  if pts_n > 0
    color 14
    text pts_x / 8, 2, pts_n
  end
end

# ---- the game ----

sub frame
  # the ship glides while a key is held
  let moved = 0
  if (down('a') || down(LEFT)) && px > 12
    call erase_ship
    px = px - 5
    moved = 1
  end
  if (down('d') || down(RIGHT)) && px < 600
    call erase_ship
    px = px + 5
    moved = 1
  end
  if moved
    call draw_ship
  end
  if down(SPACE) && bon == 0 && ticks() > shot_at
    bon = 1
    bx = px + 13
    by = 346
    shot_at = ticks() + 320
  end
  call player_shot
  call enemy_fire
  call enemy_shots
  call saucer
  # the formation steps faster the fewer invaders remain
  if ticks() >= tnext
    call step_formation
    n = base_step * (left + 8) / 58
    n = max(n - (wave - 1) * 40, 45)
    tnext = ticks() + n
  end
  if left <= 0
    wave = wave + 1
    score = score + 100
    color 15
    text 34, 12, "WAVE CLEARED"
    sleep 1200
    call start_wave
  end
end

call pick_level
while running
  call setup_level
  score = 0
  wave = 1
  over = 0
  px = 306
  shot_at = 0
  pts_n = 0
  cls
  color 8
  text 20, 24, "a/d: move (hold)   space: shoot   p: pause   q: quit"
  call start_wave
  while running && !over
    t0 = ticks()
    call frame
    k = key()
    if k == 'q' || k == ESC
      running = 0
    end
    if k == 'p'
      color 15
      text 36, 12, "PAUSED"
      k = getkey()
      color 0
      rect 280, 188, 88, 20
      call draw_formation
    end
    n = 16 - (ticks() - t0)
    if n > 0
      sleep n
    end
  end
  if running
    color 12
    text 34, 11, "GAME OVER"
    color 15
    text 29, 13, "score ", score, "   best ", best
    text 25, 15, "enter: play again   q: quit"
    k = 0
    while k != ENTER && k != 'q' && k != ESC && k != SPACE
      k = getkey()
    end
    if k != ENTER && k != SPACE
      running = 0
    end
  end
end
cls
color 15
print "Thanks for playing! best score: ", best
