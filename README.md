# 단잠 (Danjam)

**macOS only** (Apple silicon or Intel, macOS 13 or later). A member of the Dureh family.

Danjam keeps a USB fan (or any power-only USB gadget) on while your Mac is really awake with its screen on, and switches it off while the Mac sleeps and during dark wakes, the short wakes macOS makes for Wi-Fi, Power Nap or a USB event.

- **`danjamd`:** a small Rust daemon. It listens to macOS power capability changes (IOKit `IOPMConnection`):
  - before sleep: the port goes off, and only then does the Mac sleep;
  - dark wake (CPU without graphics): the port goes off, applied again 2 s and 6 s later, because macOS re-powers USB ports as it resumes;
  - user wake (graphics on): the port goes on.
- **`danjam`:** the command. `install`, `uninstall`, `status`, `test`, `log`.
- Ports are switched with [uhubctl](https://github.com/mvp/uhubctl). A port with a data device on it (shown as `connect`, for example a keyboard receiver) is never switched. No sudo is needed.

## Install

```sh
brew tap dureh-gb/tap https://github.com/dureh-gb/homebrew-tap
brew install danjam
danjam install --hub 3-1 --port 2     # the hub and port of the fan; see `uhubctl`
```

`danjam install` writes a per-user LaunchAgent (`local.danjam`) that runs `danjamd`. macOS then shows a one-time "Background Items Added" notice; keep it allowed. The log is `~/Library/Logs/danjam.log`.

## Find the port
Run `uhubctl` and look for the hub that has the port your fan is on. Then run `danjam test`: the fan should stop for 3 s. Some hubs show `ppps` but never cut the power; pick a port where `danjam test` really stops the fan.

## Remove

```sh
danjam uninstall
brew uninstall danjam
```

## Build from source

```sh
cargo build --release
install -m 755 target/release/danjamd bin/danjam ~/.local/bin/
```
