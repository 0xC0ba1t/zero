# space invaders: shoot invaders before they reach bottom
# controls: a/d or left/right to move, space to shoot, q to quit

let px = 40
let py = 22
let laser_x = 0
let laser_y = 0
let laser_active = 0

let a0_x = 10
let a0_y = 3
let a0_alive = 1
let a1_x = 25
let a1_y = 3
let a1_alive = 1
let a2_x = 40
let a2_y = 3
let a2_alive = 1
let a3_x = 55
let a3_y = 3
let a3_alive = 1
let a4_x = 70
let a4_y = 3
let a4_alive = 1

let invader_dir = 1
let frame = 0
let score = 0
let running = 1
let k = 0
let i = 0

sub draw_hud
  color 15
  text 2, 0, "Score: ", score
  text 20, 0, "SPACE INVADERS"
  text 2, 24, "a/d: move   SPACE: shoot   q: quit"
end

sub update_invaders
  # Erase invaders
  if a0_alive
    plot a0_x, a0_y, ' '
  end
  if a1_alive
    plot a1_x, a1_y, ' '
  end
  if a2_alive
    plot a2_x, a2_y, ' '
  end
  if a3_alive
    plot a3_x, a3_y, ' '
  end
  if a4_alive
    plot a4_x, a4_y, ' '
  end

  # Check edge turns
  if (a4_alive && a4_x >= 75 && invader_dir > 0) || (a0_alive && a0_x <= 4 && invader_dir < 0)
    invader_dir = -invader_dir
    a0_y = a0_y + 1
    a1_y = a1_y + 1
    a2_y = a2_y + 1
    a3_y = a3_y + 1
    a4_y = a4_y + 1
  else
    a0_x = a0_x + invader_dir
    a1_x = a1_x + invader_dir
    a2_x = a2_x + invader_dir
    a3_x = a3_x + invader_dir
    a4_x = a4_x + invader_dir
  end

  # Check invasion failure
  if (a0_alive && a0_y >= 21) || (a1_alive && a1_y >= 21) || (a2_alive && a2_y >= 21) || (a3_alive && a3_y >= 21) || (a4_alive && a4_y >= 21)
    running = 0
  end

  # Draw invaders
  color 12
  if a0_alive
    plot a0_x, a0_y, 'W'
  end
  if a1_alive
    plot a1_x, a1_y, 'W'
  end
  if a2_alive
    plot a2_x, a2_y, 'W'
  end
  if a3_alive
    plot a3_x, a3_y, 'W'
  end
  if a4_alive
    plot a4_x, a4_y, 'W'
  end
end

cls
call draw_hud

while running
  color 10
  plot px, py, '^'

  k = key()
  if k == 'q' || k == ESC
    running = 0
  end

  # Move cannon
  plot px, py, ' '
  if k == 'a' || k == 'A'
    px = max(px - 2, 2)
  end
  if k == 'd' || k == 'D'
    px = min(px + 2, 77)
  end
  color 10
  plot px, py, '^'

  # Fire Laser
  if (k == ' ' || k == ENTER) && laser_active == 0
    laser_active = 1
    laser_x = px
    laser_y = py - 1
  end

  # Laser movement & hit detection
  if laser_active
    plot laser_x, laser_y, ' '
    laser_y = laser_y - 1
    if laser_y <= 1
      laser_active = 0
    else
      color 14
      plot laser_x, laser_y, '|'

      # Check collision with invaders
      if a0_alive && laser_x == a0_x && laser_y == a0_y
        a0_alive = 0
        laser_active = 0
        score = score + 100
      end
      if a1_alive && laser_x == a1_x && laser_y == a1_y
        a1_alive = 0
        laser_active = 0
        score = score + 100
      end
      if a2_alive && laser_x == a2_x && laser_y == a2_y
        a2_alive = 0
        laser_active = 0
        score = score + 100
      end
      if a3_alive && laser_x == a3_x && laser_y == a3_y
        a3_alive = 0
        laser_active = 0
        score = score + 100
      end
      if a4_alive && laser_x == a4_x && laser_y == a4_y
        a4_alive = 0
        laser_active = 0
        score = score + 100
      end
    end
  end

  # Move invaders every 3 ticks
  if frame % 3 == 0
    call update_invaders
  end

  # Win condition
  if a0_alive == 0 && a1_alive == 0 && a2_alive == 0 && a3_alive == 0 && a4_alive == 0
    running = 0
  end

  frame = frame + 1
  call draw_hud
  sleep 40
end

cls
color 15
if a0_alive == 0 && a1_alive == 0 && a2_alive == 0 && a3_alive == 0 && a4_alive == 0
  print "VICTORY! You destroyed all alien forces. Final score: ", score
else
  print "GAME OVER! Earth was invaded. Final score: ", score
end