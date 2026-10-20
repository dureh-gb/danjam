# 단잠 (Danjam)

**macOS only** (Apple silicon or Intel, macOS 13 or later). A member of the Dureh family.

Danjam switches off an **external USB fan plugged into a front USB-C port of a Mac mini M4** while the Mac sleeps, and on again while it is awake with its screen on. The default is the **front-left** port. It also keeps the fan off during dark wakes, the short wakes macOS makes for Wi-Fi, Power Nap or a USB event; macOS powers the port back on at each of them.

- **`danjamd`:** a small Rust daemon. It listens to macOS power capability changes (IOKit `IOPMConnection`):
  - before sleep: the port goes off, and only then does the Mac sleep;
  - dark wake (CPU without graphics): the port goes off, applied again 2 s and 6 s later, because macOS re-powers USB ports as it resumes;
  - user wake (graphics on): the port goes on.
- **`danjam`:** the command. `install`, `uninstall`, `status`, `test`, `log`.
- Ports are switched with [uhubctl](https://github.com/mvp/uhubctl). A port with a data device on it (shown as `connect`, for example a keyboard receiver) is never switched. No sudo is needed.

## Install with Homebrew

**Before you start**
- macOS on Apple silicon. Homebrew refuses other systems.
- The formula installs a prebuilt arm64 `danjamd` (no Rust needed) and `uhubctl` as a dependency.

**Install**

```sh
brew tap dureh-gb/tap
brew trust --formula dureh-gb/tap/danjam    # Homebrew asks you to trust a formula from a new tap once
brew install danjam
```

`brew install dureh-gb/tap/danjam` works as well once the tap is added and trusted. `brew trust dureh-gb/tap` trusts every formula in the tap instead.

**Start it for your fan** (front-left port by default):

```sh
danjam install                 # or: danjam install --front right
danjam test                    # the fan should stop for 3 s
danjam status
```

`danjam install` writes a per-user LaunchAgent (`local.danjam`) that runs `danjamd`. macOS then shows a one-time "Background Items Added" notice; keep it allowed. The log is `~/Library/Logs/danjam.log`.

**Upgrade**

```sh
brew update
brew upgrade danjam
danjam install                 # restarts the LaunchAgent with the new danjamd
```

## Ports
- **Mac mini M4 front USB-C:** hub `3-1` (its USB3 half is `3-2`, switched together). Front left = port 2 (default), front right = port 1.
- **Other ports:** `danjam install --hub <hub> --port <n>` (see `uhubctl`). Many hubs, monitor hubs included, report `ppps` but never cut the power, so check with `danjam test`.

## Remove

```sh
danjam uninstall               # stops it, removes the LaunchAgent; the log stays
brew uninstall danjam
brew untap dureh-gb/tap        # optional
```

## Build from source

```sh
cargo build --release
install -m 755 target/release/danjamd bin/danjam ~/.local/bin/
```
