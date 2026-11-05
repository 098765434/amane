/*
 * Android phones through adb: the ones it has (over USB or the network),
 * the phones on the network offering Wireless debugging, and pairing and
 * connecting them; adb tells of every change itself, so nothing is polled
 */

mod devices;
mod discovery;

use std::io::Read;
use std::net::IpAddr;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::Service;

pub use devices::{AdbDevice, AdbState};
pub use discovery::{WirelessKind, WirelessService};

// what `adb` is run as; the shell's PATH has it when android-tools is installed
const ADB: &str = "adb";

#[derive(Default, PartialEq)]
pub struct Adb {
    installed: bool,
    devices: Vec<AdbDevice>,

    // what Wireless debugging announces on the network, while something looks
    found: Vec<WirelessService>,

    // the addresses to stay connected to while they offer Wireless debugging
    wanted: Vec<IpAddr>,

    // a pairing or connection under way: where to
    busy: Option<String>,

    // why the last pairing or connection failed, cleared by the next that works
    error: Option<String>,
}

impl Service for Adb {
    fn new() -> Self {
        Self {
            installed: installed(),
            ..Self::default()
        }
    }

    // adb writes the whole list again on each change, until its server goes away
    fn listen() {
        if !Self::read().installed {
            return;
        }

        loop {
            track();

            // the server stopped (killed, or adb updated); start it again a little later
            thread::sleep(Duration::from_secs(5));
        }
    }
}

impl Adb {
    pub fn installed(&self) -> bool {
        self.installed
    }

    pub fn devices(&self) -> &[AdbDevice] {
        &self.devices
    }

    // a phone adb can use now, at this network address
    pub fn ready_at(&self, address: IpAddr) -> Option<&AdbDevice> {
        self.devices
            .iter()
            .find(|device| device.ready() && device.address() == Some(address))
    }

    // phones adb can use now, over USB
    pub fn ready_usb(&self) -> impl Iterator<Item = &AdbDevice> {
        self.devices
            .iter()
            .filter(|device| device.ready() && device.address().is_none())
    }

    pub fn found(&self) -> &[WirelessService] {
        &self.found
    }

    pub fn busy(&self) -> Option<&str> {
        self.busy.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /*
     * keeps a phone at `address` connected: whenever its Wireless debugging
     * shows on the network and adb doesn't have it, it's connected; the port
     * changes each time Wireless debugging is turned on, so it's found anew
     */
    pub fn want(address: IpAddr) {
        let mut adb = Self::write();

        if !adb.wanted.contains(&address) {
            adb.wanted.push(address);
        }

        drop(adb);

        discovery::start();
    }

    pub fn unwant(address: IpAddr) {
        let mut adb = Self::write();

        adb.wanted.retain(|wanted| *wanted != address);

        let looking = !adb.wanted.is_empty();

        drop(adb);

        if !looking {
            discovery::stop();
        }
    }

    // looks for phones offering pairing too, like while a pairing code is shown
    pub fn look(on: bool) {
        if on {
            discovery::start();
        } else if Self::read().wanted.is_empty() {
            discovery::stop();
        }
    }

    // pairs with the code Wireless debugging shows, or the one a QR code carried
    pub fn pair(address: IpAddr, port: u16, code: &str) {
        let target = host(address, port);
        let code = String::from(code);

        in_background(format!("Pairing with {target}…"), move || {
            run(None, &["pair", &target, &code]).and_then(|said| {
                // adb exits 0 even when pairing fails
                if said.contains("Successfully paired") {
                    Ok(())
                } else {
                    Err(said)
                }
            })
        });
    }

    pub fn connect(address: IpAddr, port: u16) {
        let target = host(address, port);

        in_background(format!("Connecting to {target}…"), move || {
            connect(&target)
        });
    }

    pub fn disconnect(serial: &str) {
        let serial = String::from(serial);

        in_background(format!("Disconnecting {serial}…"), move || {
            run(None, &["disconnect", &serial]).map(|_| ())
        });
    }
}

/*
 * runs adb for one phone (by serial) or for adb itself, waiting for it;
 * what it printed, or why it failed
 */
pub fn run(serial: Option<&str>, arguments: &[&str]) -> Result<String, String> {
    let mut command = Command::new(ADB);

    if let Some(serial) = serial {
        command.args(["-s", serial]);
    }

    let output = command
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("adb didn't start: {error}"))?;

    let said = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if output.status.success() {
        return Ok(said);
    }

    let complaint = String::from_utf8_lossy(&output.stderr).trim().to_string();

    Err(if complaint.is_empty() {
        said
    } else {
        complaint
    })
}

fn installed() -> bool {
    Command::new(ADB)
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn host(address: IpAddr, port: u16) -> String {
    match address {
        IpAddr::V4(address) => format!("{address}:{port}"),
        IpAddr::V6(address) => format!("[{address}]:{port}"),
    }
}

fn connect(target: &str) -> Result<(), String> {
    let said = run(None, &["connect", target])?;

    // like pair, connect exits 0 when it fails
    if said.contains("connected to") {
        Ok(())
    } else {
        Err(said)
    }
}

// a slow adb call on its own thread, so the worker's other calls don't wait seconds for it
fn in_background(doing: String, job: impl FnOnce() -> Result<(), String> + Send + 'static) {
    Adb::write().busy = Some(doing);

    thread::spawn(move || {
        let result = job();

        let mut adb = Adb::write();

        adb.busy = None;
        adb.error = result.err();
    });
}

// one run of `adb track-devices`, until the server goes away
fn track() {
    let Ok(mut child) = Command::new(ADB)
        .args(["track-devices", "-l"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };

    let Some(mut stdout) = child.stdout.take() else {
        return;
    };

    loop {
        // each list comes as four hex digits of length, then that many bytes
        let mut length = [0u8; 4];

        if stdout.read_exact(&mut length).is_err() {
            break;
        }

        let Some(length) = std::str::from_utf8(&length)
            .ok()
            .and_then(|length| usize::from_str_radix(length, 16).ok())
        else {
            break;
        };

        let mut list = vec![0u8; length];

        if stdout.read_exact(&mut list).is_err() {
            break;
        }

        let devices = devices::parse(&String::from_utf8_lossy(&list));

        let mut adb = Adb::write();

        if adb.devices == devices {
            adb.quiet();
        } else {
            adb.devices = devices;
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    Adb::write().devices.clear();
}

// called by discovery as services come and go
fn found(services: Vec<WirelessService>) {
    let mut adb = Adb::write();

    if adb.found == services {
        adb.quiet();

        return;
    }

    adb.found = services;

    // a wanted phone offering a connection that adb doesn't have yet
    let missing: Vec<(IpAddr, u16)> = adb
        .found
        .iter()
        .filter(|service| service.kind == WirelessKind::Connect)
        .filter(|service| adb.wanted.contains(&service.address))
        .filter(|service| {
            !adb.devices
                .iter()
                .any(|device| device.address() == Some(service.address))
        })
        .map(|service| (service.address, service.port))
        .collect();

    let idle = adb.busy.is_none();

    drop(adb);

    if idle {
        for (address, port) in missing {
            Adb::connect(address, port);
        }
    }
}
