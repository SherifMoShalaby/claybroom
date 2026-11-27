//! Fan speeds and graphics-chip use for the System monitor and the menu-bar panel.
//! macOS: fans from the SMC sensor chip (read-only,
//! through IOKit's public AppleSMC connection, as monitoring apps do) and GPU use from the
//! graphics driver's statistics (`ioreg`). Linux: /sys hwmon fans and the GPU's busy percent.
//! NVIDIA cards anywhere: `nvidia-smi`. Anything a computer doesn't share is simply left out.

use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fan {
    pub rpm: f32,
    /// The fan's top speed, when known.
    pub max: Option<f32>,
}

impl Fan {
    pub fn percent(&self) -> Option<f32> {
        self.max.filter(|m| *m > 0.0).map(|m| (self.rpm / m * 100.0).clamp(0.0, 100.0))
    }
}

/// How busy the graphics chip is, 0–100.
pub fn gpu_usage() -> Option<f32> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("ioreg").args(["-r", "-d", "1", "-c", "IOAccelerator"]).output().ok()?;
        if let Some(v) = parse_ioreg_gpu(&String::from_utf8_lossy(&out.stdout)) {
            return Some(v);
        }
    }
    #[cfg(target_os = "linux")]
    {
        for e in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
            if let Ok(s) = std::fs::read_to_string(e.path().join("device/gpu_busy_percent")) {
                if let Ok(v) = s.trim().parse::<f32>() {
                    return Some(v);
                }
            }
        }
    }
    nvidia_smi()
}

/// "Device Utilization %"=12 in ioreg's accelerator statistics (the busiest chip if several).
#[cfg(any(target_os = "macos", test))]
pub fn parse_ioreg_gpu(text: &str) -> Option<f32> {
    text.match_indices("\"Device Utilization %\"=")
        .filter_map(|(i, k)| text[i + k.len()..].split(|c: char| !c.is_ascii_digit()).next()?.parse::<f32>().ok())
        .reduce(f32::max)
}

fn nvidia_smi() -> Option<f32> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args(["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"]);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    let out = cmd.output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse::<f32>().ok()).reduce(f32::max)
}

/// Every fan the computer reports.
pub fn fans() -> Vec<Fan> {
    #[cfg(target_os = "macos")]
    {
        smc::fans()
    }
    #[cfg(target_os = "linux")]
    {
        let mut out = vec![];
        for hw in std::fs::read_dir("/sys/class/hwmon").into_iter().flatten().flatten() {
            for n in 1..=8 {
                let read = |f: &str| std::fs::read_to_string(hw.path().join(format!("fan{n}_{f}"))).ok()?.trim().parse::<f32>().ok();
                if let Some(rpm) = read("input") {
                    out.push(Fan { rpm, max: read("max") });
                }
            }
        }
        out
    }
    #[cfg(windows)]
    {
        vec![]
    }
}

#[cfg(target_os = "macos")]
mod smc {
    //! Reading keys from Apple's SMC (System Management Controller) with IOKit: open the
    //! "AppleSMC" service, ask for a key's type and size, then for its bytes. Read-only.
    use super::Fan;
    use std::ffi::{c_char, c_void};

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Vers {
        major: u8,
        minor: u8,
        build: u8,
        reserved: u8,
        release: u16,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct PLimit {
        version: u16,
        length: u16,
        cpu: u32,
        gpu: u32,
        mem: u32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct KeyInfo {
        size: u32,
        kind: u32,
        attributes: u8,
    }
    /// The SMC's request and answer (80 bytes, the layout IOKit expects).
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub(super) struct KeyData {
        key: u32,
        vers: Vers,
        plimit: PLimit,
        info: KeyInfo,
        result: u8,
        status: u8,
        data8: u8,
        data32: u32,
        bytes: [u8; 32],
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
        fn IOServiceOpen(service: u32, owning_task: u32, kind: u32, connect: *mut u32) -> i32;
        fn IOServiceClose(connect: u32) -> i32;
        fn IOObjectRelease(object: u32) -> i32;
        fn IOConnectCallStructMethod(connect: u32, selector: u32, input: *const c_void, input_size: usize, output: *mut c_void, output_size: *mut usize) -> i32;
    }
    unsafe extern "C" {
        static mach_task_self_: u32;
    }

    const READ_BYTES: u8 = 5;
    const READ_INFO: u8 = 9;
    const HANDLE_SMC: u32 = 2;

    fn fourcc(s: &str) -> u32 {
        s.bytes().fold(0, |a, b| (a << 8) | b as u32)
    }

    struct Conn(u32);
    impl Drop for Conn {
        fn drop(&mut self) {
            // SAFETY: closing the connection IOServiceOpen gave.
            unsafe { IOServiceClose(self.0) };
        }
    }

    fn open() -> Option<Conn> {
        // SAFETY: plain IOKit calls with a C string and out-pointers that live through the call.
        unsafe {
            let service = IOServiceGetMatchingService(0, IOServiceMatching(c"AppleSMC".as_ptr()));
            if service == 0 {
                return None;
            }
            let mut conn = 0u32;
            let ok = IOServiceOpen(service, mach_task_self_, 0, &mut conn);
            IOObjectRelease(service);
            (ok == 0 && conn != 0).then_some(Conn(conn))
        }
    }

    fn call(c: &Conn, input: &KeyData) -> Option<KeyData> {
        let mut out = KeyData::default();
        let mut size = std::mem::size_of::<KeyData>();
        // SAFETY: both structs have the layout the SMC expects and outlive the call.
        let r = unsafe { IOConnectCallStructMethod(c.0, HANDLE_SMC, input as *const _ as *const c_void, size, &mut out as *mut _ as *mut c_void, &mut size) };
        (r == 0 && out.result == 0).then_some(out)
    }

    /// A key's value as a number, whatever its SMC type.
    fn read(c: &Conn, key: &str) -> Option<f32> {
        let info = call(c, &KeyData { key: fourcc(key), data8: READ_INFO, ..Default::default() })?.info;
        let out = call(c, &KeyData { key: fourcc(key), info, data8: READ_BYTES, ..Default::default() })?;
        decode(info.kind, &out.bytes[..(info.size as usize).min(32)])
    }

    /// SMC number formats: "flt " (Apple silicon), "fpe2" (Intel fans), unsigned integers.
    pub(super) fn decode(kind: u32, b: &[u8]) -> Option<f32> {
        Some(match kind {
            k if k == fourcc("flt ") => f32::from_le_bytes(b.get(..4)?.try_into().ok()?),
            k if k == fourcc("fpe2") => u16::from_be_bytes(b.get(..2)?.try_into().ok()?) as f32 / 4.0,
            k if k == fourcc("ui8 ") => *b.first()? as f32,
            k if k == fourcc("ui16") => u16::from_be_bytes(b.get(..2)?.try_into().ok()?) as f32,
            k if k == fourcc("ui32") => u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as f32,
            _ => return None,
        })
    }

    pub fn fans() -> Vec<Fan> {
        let Some(c) = open() else { return vec![] };
        let n = read(&c, "FNum").unwrap_or(0.0) as usize;
        (0..n.min(8))
            .filter_map(|i| Some(Fan { rpm: read(&c, &format!("F{i}Ac"))?, max: read(&c, &format!("F{i}Mx")) }))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn request_has_the_size_iokit_expects() {
            assert_eq!(std::mem::size_of::<super::KeyData>(), 80);
        }

        #[test]
        fn smc_numbers_decode() {
            assert_eq!(super::decode(super::fourcc("flt "), &1200.5f32.to_le_bytes()), Some(1200.5));
            assert_eq!(super::decode(super::fourcc("fpe2"), &[0x12, 0xc0]), Some(1200.0));
            assert_eq!(super::decode(super::fourcc("ui8 "), &[2]), Some(2.0));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ioreg_gpu_use_is_read() {
        let s = r#""PerformanceStatistics" = {"Tiler Utilization %"=3,"Device Utilization %"=27,"Renderer Utilization %"=20}"#;
        assert_eq!(super::parse_ioreg_gpu(s), Some(27.0));
        assert_eq!(super::parse_ioreg_gpu("nothing here"), None);
    }

    #[test]
    fn fan_percent_needs_a_top_speed() {
        assert_eq!(super::Fan { rpm: 3000.0, max: Some(6000.0) }.percent(), Some(50.0));
        assert_eq!(super::Fan { rpm: 3000.0, max: None }.percent(), None);
    }

    /// Read-only: what this computer reports. `cargo test sensors -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_computer() {
        println!("GPU use: {:?}", super::gpu_usage());
        println!("fans: {:?}", super::fans());
    }
}
