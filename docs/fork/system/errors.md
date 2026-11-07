# Service errors (fork addition)

One way for the fork's services to say something failed: `ServiceError { kind, message, at }`.

| Field | What it is |
|---|---|
| `kind: ErrorKind` | what kind of failure, for the UI to word: `Unreachable`, `AuthenticationFailed`, `Rejected`, `Canceled`, `TimedOut`, `NotReady`, `Blocked`, `NotSupported`, `Gone`, `PermissionDenied`, `Unavailable`, `Failed` (`#[non_exhaustive]`) |
| `message: String` | the other side's own words, untranslated, for logs and as a last resort; may be empty |
| `at: Instant` | when it happened |

`ServiceError::new(kind, message)` stamps `at`. It converts from `io::Error`, `BusError` and
`zbus::Error`. `kind_of_bus(name)` maps the standard D-Bus error names, for a service that maps
its own names (BlueZ, NetworkManager) first. `Display` is the message, or the kind without one.

## The convention

- **Failures.** A service keeps its last failure per thing it acts on (a device, a network, a
  request), readable with `failure(key) -> Option<&ServiceError>`. The failure stays until that
  thing is tried again. amane never clears one on a timer: how long it shows is the UI's choice,
  from `at`, and so are the words, from `kind`.
- **Work in progress.** `operation(key) -> Option<Op>`, with an `Op` enum of the service's own
  (Bluetooth's `Connecting`, `Pairing` and so on).
- **Blocking calls** return `Result<_, ServiceError>`.

Upstream services (`Lock`, `Pam`) keep their own error types, so a rebase doesn't conflict there.

## For services outside amane

A program's own `Service` can use the same pieces amane's do:

| Item | What it is for |
|---|---|
| `amane::run_in_order(job)` | the one ordered thread for control calls, so input handlers never wait on a slow bus |
| `Service::watched_within(idle)` | whether something watched the service lately, for a resync that runs only then |
| `Service::peek()` | a read for the service's own listener, which doesn't count as watching and redraws nothing |
| `amane::IDLE_STOP` | the 30 s amane's own services use for both |

Code: `src/services/error.rs`. Tests: the kinds for the standard D-Bus names and `io` errors.
