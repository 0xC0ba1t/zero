# counts down from 5, one number every half second
let n = 5
while n > 0
  print n, "..."
  sleep 500
  n = n - 1
end
print "liftoff!"
