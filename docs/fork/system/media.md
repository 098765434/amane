# Media: players followed by their signals (fork change)

Upstream asks the session bus for every name and reads each player again once a second. The fork
follows the players' signals instead, so with no player open the service never wakes.

## How it works

- **Signals** (`src/services/media/signals.rs`):
  - `NameOwnerChanged` for names under `org.mpris.MediaPlayer2`, as players open and quit;
  - `PropertiesChanged` on `/org/mpris/MediaPlayer2`: a new track, play and pause, the rate;
  - `Seeked`.

  A burst of signals (a track change sends several) counts as one change: the players are read
  again 50 ms after the last one. The read happens outside the service's lock.
- **Position.** Players don't send the position while a track plays. `MediaPlayer::position()`
  works it out from the last reading, the playback rate and the time since, and stops at the
  track's length. A view that shows the position redraws on its own timer while playing.
- The choice of the active player is unchanged: the one playing, else the one shown last, else
  the first.

Code: `src/services/media.rs`, `src/services/media/{player,signals}.rs`. Tests: the position
from the last reading at normal and double speed, paused, and past the end.
