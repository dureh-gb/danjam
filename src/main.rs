//! danjamd (단잠, "sweet sleep"): keep a USB fan on only while this Mac is really awake (its graphics are on), and off while it sleeps
//! or is in a dark wake.
//!
//! macOS tells a power-management client (IOPMConnection, exported by IOKit) every time the system's capabilities
//! change: before sleep (capabilities drop to none), at a dark wake (CPU without Graphics/Video) and at a user wake
//! (Graphics back). The event is acknowledged only after the fan port has been switched, so macOS waits for the fan
//! to be off before it sleeps. At a dark wake macOS also re-powers the USB ports by itself, sometimes after the event,
//! so "off" is applied again a few seconds later while the Mac is still dark.
//!
//! The port is switched with Homebrew's `uhubctl`. A port with a data device on it (uhubctl shows "connect", e.g. a
//! keyboard receiver) is never switched.
//!
//!     danjamd [--hub 3-1] [--port 2] [--log ~/Library/Logs/danjam.log]

use std::ffi::{c_void, CString};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::raw::{c_char, c_int, c_long};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

type CFStringRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type IOPMConnection = *mut c_void;
type IOReturn = c_int;
type Token = u32;
type Caps = u32;
type Handler = extern "C" fn(*mut c_void, IOPMConnection, Token, Caps);

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> CFStringRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRun();
    static kCFRunLoopDefaultMode: CFStringRef;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMConnectionCreate(name: CFStringRef, interests: Caps, conn: *mut IOPMConnection) -> IOReturn;
    fn IOPMConnectionSetNotification(conn: IOPMConnection, param: *mut c_void, handler: Handler) -> IOReturn;
    fn IOPMConnectionScheduleWithRunLoop(conn: IOPMConnection, rl: CFRunLoopRef, mode: CFStringRef) -> IOReturn;
    fn IOPMConnectionAcknowledgeEvent(conn: IOPMConnection, token: Token) -> IOReturn;
    fn IOPMConnectionGetSystemCapabilities() -> Caps;
}

// time.h, for log time stamps in local time.
#[repr(C)]
struct Tm {
    sec: c_int, min: c_int, hour: c_int, mday: c_int, mon: c_int, year: c_int, wday: c_int, yday: c_int,
    isdst: c_int, gmtoff: c_long, zone: *const c_char,
}
extern "C" {
    fn time(t: *mut c_long) -> c_long;
    fn localtime_r(t: *const c_long, out: *mut Tm) -> *mut Tm;
}

// IOPMCapabilityBits (IOKit/pwr_mgt/IOPMLibPrivate.h).
const CAP_CPU: Caps = 0x1;
const CAP_VIDEO: Caps = 0x2;
const CAP_AUDIO: Caps = 0x4;
const CAP_NETWORK: Caps = 0x8;
const CAP_DISK: Caps = 0x10;
const CAP_MASK: Caps = 0xffff;
const UTF8: u32 = 0x0800_0100;
const UHUBCTL: &str = "/opt/homebrew/bin/uhubctl";

struct Config {
    hub: String,
    port: String,
    log: String,
}
static CONFIG: OnceLock<Config> = OnceLock::new();
static SWITCH: Mutex<()> = Mutex::new(());

fn cfg() -> &'static Config {
    CONFIG.get().expect("config")
}

fn stamp() -> String {
    unsafe {
        let mut now: c_long = 0;
        time(&mut now);
        let mut tm: Tm = std::mem::zeroed();
        localtime_r(&now, &mut tm);
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", tm.year + 1900, tm.mon + 1, tm.mday, tm.hour, tm.min, tm.sec)
    }
}

fn log(msg: &str) {
    let line = format!("{} daemon: {msg}\n", stamp());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&cfg().log) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// What a set of capabilities means for the fan.
#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Sleep,
    DarkWake,
    UserWake,
}

fn state_of(caps: Caps) -> State {
    let c = caps & CAP_MASK;
    if c & CAP_CPU == 0 {
        State::Sleep
    } else if c & CAP_VIDEO == 0 {
        State::DarkWake
    } else {
        State::UserWake
    }
}

/// The uhubctl line of `port` inside the section of `hub` (uhubctl also lists the USB3 twin of a front port).
fn port_line(listing: &str, hub: &str, port: &str) -> Option<String> {
    let mut in_hub = false;
    let want = format!("Port {port}:");
    for line in listing.lines() {
        if line.contains("status for hub") {
            in_hub = line.contains(&format!("hub {hub} "));
        } else if in_hub && line.trim_start().starts_with(&want) {
            return Some(line.trim().to_string());
        }
    }
    None
}

/// Switch the fan port; refuses a port with a data device on it. Returns a short result for the log.
fn switch(on: bool) -> String {
    let _guard = SWITCH.lock().unwrap_or_else(|e| e.into_inner());
    let c = cfg();
    let listing = Command::new(UHUBCTL).args(["-l", &c.hub, "-p", &c.port]).output();
    let listing = match listing {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(e) => return format!("uhubctl did not run ({e})"),
    };
    match port_line(&listing, &c.hub, &c.port) {
        None => return format!("hub {} port {} not found, not switched", c.hub, c.port),
        Some(l) if l.contains("connect") => return format!("hub {} port {} has a data device ({l}), not switched", c.hub, c.port),
        Some(_) => {}
    }
    let action = if on { "on" } else { "off" };
    match Command::new(UHUBCTL).args(["-l", &c.hub, "-p", &c.port, "-a", action]).output() {
        Ok(o) if o.status.success() => format!("port {} {action} ok", c.port),
        Ok(o) => format!("port {} {action} failed (exit {:?})", c.port, o.status.code()),
        Err(e) => format!("port {} {action} failed ({e})", c.port),
    }
}

fn apply(state: State, why: &str) {
    let result = switch(state == State::UserWake);
    log(&format!("{why}: {state:?} -> {result}"));
    if state == State::DarkWake {
        // macOS may re-power the port after this event while the USB controller resumes: apply "off" again.
        thread::spawn(|| {
            for delay in [2u64, 6] {
                thread::sleep(Duration::from_secs(delay));
                let now = state_of(unsafe { IOPMConnectionGetSystemCapabilities() });
                if now != State::DarkWake {
                    return;
                }
                let r = switch(false);
                log(&format!("dark wake, again after {delay} s: {r}"));
            }
        });
    }
}

extern "C" fn on_event(_param: *mut c_void, conn: IOPMConnection, token: Token, caps: Caps) {
    apply(state_of(caps), &format!("event caps=0x{:x}", caps & CAP_MASK));
    // Only now: before a sleep, macOS waits for this, so the fan is off before the Mac sleeps.
    unsafe {
        IOPMConnectionAcknowledgeEvent(conn, token);
    }
}

fn parse_args() -> Config {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut c = Config { hub: "3-1".into(), port: "2".into(), log: format!("{home}/Library/Logs/danjam.log") };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().unwrap_or_else(|| usage(&format!("{a} needs a value")));
        match a.as_str() {
            "--hub" => c.hub = v,
            "--port" => c.port = v,
            "--log" => c.log = v,
            _ => usage(&format!("unknown option {a}")),
        }
    }
    c
}

fn usage(msg: &str) -> ! {
    eprintln!("danjamd: {msg}\nusage: danjamd [--hub 3-1] [--port 2] [--log <file>]");
    std::process::exit(2)
}

fn main() {
    let _ = CONFIG.set(parse_args());
    unsafe {
        let name = CString::new("danjamd").unwrap();
        let cf_name = CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), UTF8);
        let mut conn: IOPMConnection = std::ptr::null_mut();
        let interests = CAP_CPU | CAP_VIDEO | CAP_AUDIO | CAP_NETWORK | CAP_DISK;
        let r = IOPMConnectionCreate(cf_name, interests, &mut conn);
        if r != 0 || conn.is_null() {
            log(&format!("IOPMConnectionCreate failed (0x{r:x}); exiting"));
            std::process::exit(1);
        }
        let r = IOPMConnectionSetNotification(conn, std::ptr::null_mut(), on_event);
        if r != 0 {
            log(&format!("IOPMConnectionSetNotification failed (0x{r:x}); exiting"));
            std::process::exit(1);
        }
        let r = IOPMConnectionScheduleWithRunLoop(conn, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
        if r != 0 {
            log(&format!("IOPMConnectionScheduleWithRunLoop failed (0x{r:x}); exiting"));
            std::process::exit(1);
        }
        let caps = IOPMConnectionGetSystemCapabilities();
        let c = cfg();
        log(&format!("started (hub {} port {}, pid {})", c.hub, c.port, std::process::id()));
        apply(state_of(caps), &format!("start caps=0x{:x}", caps & CAP_MASK));
        CFRunLoopRun();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_map_to_states() {
        assert_eq!(state_of(0), State::Sleep);
        assert_eq!(state_of(CAP_CPU | CAP_NETWORK | CAP_DISK), State::DarkWake); // [CDN]
        assert_eq!(state_of(0x1f), State::UserWake); // [CDNVA]
        assert_eq!(state_of(CAP_VIDEO), State::Sleep, "no CPU = sleep, whatever else");
    }

    #[test]
    fn port_line_reads_only_the_named_hub() {
        let listing = "Current status for hub 3-2 [05ac:800c Apple USB3 Gen2 Hub, USB 3.20, 2 ports, ppps]\n  Port 2: 02a0 power 5gbps Rx.Detect\nCurrent status for hub 3-1 [05ac:800b Apple USB2 Hub, USB 2.10, 2 ports, ppps]\n  Port 1: 0103 power enable connect [046d:c548 Logitech USB Receiver]\n  Port 2: 0100 power\n";
        assert_eq!(port_line(listing, "3-1", "2").as_deref(), Some("Port 2: 0100 power"));
        assert!(port_line(listing, "3-1", "1").unwrap().contains("connect"));
        assert_eq!(port_line(listing, "3-1", "4"), None);
        assert_eq!(port_line(listing, "0-1", "2"), None);
    }
}
