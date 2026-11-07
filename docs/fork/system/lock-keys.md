# Caps Lock and Num Lock: `LockKeys` (fork addition)

```rust,ignore
use amane::{LockKeys, Service};

LockKeys::subscribe(|keys| show_caps_lock(keys.caps_lock())).forever();
```

`caps_lock()` and `num_lock()`. On the niri fork with the `lock-keys` feature they come from its
`KeyboardLockKeysChanged` event on the shared event stream, so nothing polls. Elsewhere the
keyboard LEDs (`/sys/class/leds/*::capslock`, `*::numlock`; any keyboard's counts) are read
every 250 ms, only while watched: LED brightness sends no uevent.

Code: `src/services/lock_keys.rs`.
