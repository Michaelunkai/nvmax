//! nvmax: installs the latest NVIDIA Control Panel and forces every NVIDIA
//! driver profile setting it can reach to maximum performance. Pure Rust, std only.
//! Idempotent: every run re-applies, re-reads and prints a verification table.

use std::ffi::c_void;
use std::process::{exit, Command};
use std::thread::sleep;
use std::time::Duration;

const NVCP_STORE_ID: &str = "9NF8H0H7WMLT";
const ATTEMPTS: u32 = 3;

// ---------- tiny FFI (kernel32) ----------
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}


// ---------- NVIDIA Profile Inspector (embedded) for settings the driver DB does not expose to NVAPI set ----------
// Low Latency Mode (Ultra), CUDA sysmem fallback and Force P2 are stored in the base profile but plain
// DRS_SetSetting returns -160 for them; NVIDIA Profile Inspector (MIT) writes them. No panel, no clicks.
const NPI_EXE: &[u8] = include_bytes!("../npi/nvidiaProfileInspector.exe");
const NPI_CFG: &[u8] = include_bytes!("../npi/nvidiaProfileInspector.exe.config");
const NPI_SETTINGS: [(u32, u32, &str); 4] = [
    (0x10ECECC9, 1, "CUDA Sysmem Fallback Policy = Prefer no sysmem fallback"),
    (0x0005F543, 2, "Low Latency Mode (panel state) = Ultra"),
    (0x10835000, 1, "Ultra Low Latency enabled"),
    (0x50166C5E, 0, "CUDA Force P2 State = Off"),
];

fn stored_values() -> Vec<(u32, u32)> {
    let Ok(nv) = nv_open() else { return vec![] };
    let m = unsafe { LoadLibraryA(c"nvapi64.dll".as_ptr().cast()) };
    let qp = unsafe { GetProcAddress(m, c"nvapi_QueryInterface".as_ptr().cast()) };
    let q: QueryIface = unsafe { std::mem::transmute(qp) };
    type EnumFn = unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut u32, *mut Setting) -> Status;
    let Some(en) = (unsafe { iface::<EnumFn>(q, 0xCFD6983E) }) else { return vec![] };
    let mut out = vec![];
    for idx in 0..400u32 {
        let mut one = Nv::blank(0);
        let mut cnt = 1u32;
        let st = unsafe { en(nv.session, nv.profile, idx, &mut cnt, &mut *one) };
        if st == 0 && cnt > 0 {
            out.push((one.id, one.cur[0]));
        }
    }
    out
}

fn npi_rows() -> Vec<(bool, String)> {
    let have = stored_values();
    NPI_SETTINGS
        .iter()
        .map(|&(id, want, name)| {
            let got = have.iter().find(|(i, _)| *i == id).map(|(_, v)| *v);
            let ok = got == Some(want) || (got.is_none() && want == 0);
            (ok, format!("{name}: {}", got.map_or("unset".into(), |v| v.to_string())))
        })
        .collect()
}

fn step_npi() -> Vec<(bool, String)> {
    let dir = std::env::temp_dir().join("nvmax-npi");
    let _ = std::fs::create_dir_all(&dir);
    let exe = dir.join("nvidiaProfileInspector.exe");
    let nip = dir.join("nvmax.nip");
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-16\"?><ArrayOfProfile><Profile><ProfileName>Base Profile</ProfileName><Executeables /><Settings>");
    for (id, v, _) in NPI_SETTINGS {
        xml.push_str(&format!("<ProfileSetting><SettingNameInfo /><SettingID>{id}</SettingID><SettingValue>{v}</SettingValue><ValueType>Dword</ValueType></ProfileSetting>"));
    }
    xml.push_str("</Settings></Profile></ArrayOfProfile>");
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let wrote = std::fs::write(&exe, NPI_EXE).is_ok()
        && std::fs::write(dir.join("nvidiaProfileInspector.exe.config"), NPI_CFG).is_ok()
        && std::fs::write(&nip, bytes).is_ok();
    if !wrote {
        return vec![(false, "NVIDIA Profile Inspector could not be written to temp".into())];
    }
    let mut rows = vec![];
    for _ in 0..ATTEMPTS {
        let _ = Command::new(&exe).arg("-silentImport").arg(&nip).output();
        rows = npi_rows();
        if rows.iter().all(|r| r.0) {
            break;
        }
        sleep(Duration::from_secs(2));
    }
    let _ = std::fs::remove_dir_all(&dir);
    rows
}


// ---------- NVAPI ----------
type Status = i32;
type QueryIface = unsafe extern "C" fn(u32) -> *mut c_void;

#[repr(C)]
struct Setting {
    version: u32,
    name: [u16; 2048],
    id: u32,
    ty: u32,
    loc: u32,
    is_cur_pre: u32,
    is_pre_valid: u32,
    pre: [u32; 1025],
    cur: [u32; 1025],
}
const SETTING_VER: u32 = (std::mem::size_of::<Setting>() as u32) | (1 << 16);

struct Nv {
    session: *mut c_void,
    profile: *mut c_void,
    set: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut Setting) -> Status,
    get: unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut Setting) -> Status,
    save_fn: unsafe extern "C" fn(*mut c_void) -> Status,
    name_of: Option<unsafe extern "C" fn(u32, *mut u16) -> Status>,
    destroy: unsafe extern "C" fn(*mut c_void) -> Status,
}

unsafe fn iface<T>(q: QueryIface, id: u32) -> Option<T> {
    let p = unsafe { q(id) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }
}

fn nv_open() -> Result<Nv, String> {
    unsafe {
        let m = LoadLibraryA(c"nvapi64.dll".as_ptr().cast());
        if m.is_null() {
            return Err("nvapi64.dll not found".into());
        }
        let qp = GetProcAddress(m, c"nvapi_QueryInterface".as_ptr().cast());
        if qp.is_null() {
            return Err("nvapi_QueryInterface missing".into());
        }
        let q: QueryIface = std::mem::transmute(qp);
        let init: unsafe extern "C" fn() -> Status = iface(q, 0x0150E828).ok_or("Initialize")?;
        let create: unsafe extern "C" fn(*mut *mut c_void) -> Status =
            iface(q, 0x0694D52E).ok_or("DRS_CreateSession")?;
        let load: unsafe extern "C" fn(*mut c_void) -> Status =
            iface(q, 0x375DBD6B).ok_or("DRS_LoadSettings")?;
        let base: unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> Status =
            iface(q, 0xDA8466A0).ok_or("DRS_GetBaseProfile")?;
        let set = iface(q, 0x577DD202).ok_or("DRS_SetSetting")?;
        let get = iface(q, 0x73BF8338).ok_or("DRS_GetSetting")?;
        let save = iface(q, 0xFCBC7E14).ok_or("DRS_SaveSettings")?;
        let destroy = iface(q, 0xDAD9CFF8).ok_or("DRS_DestroySession")?;
        let name_of = iface(q, 0xD61CBE6E);
        let st = init();
        if st != 0 {
            return Err(format!("NvAPI_Initialize status {st}"));
        }
        let mut session = std::ptr::null_mut();
        let st = create(&mut session);
        if st != 0 {
            return Err(format!("DRS_CreateSession status {st}"));
        }
        let st = load(session);
        if st != 0 {
            return Err(format!("DRS_LoadSettings status {st}"));
        }
        let mut profile = std::ptr::null_mut();
        let st = base(session, &mut profile);
        if st != 0 {
            return Err(format!("DRS_GetBaseProfile status {st}"));
        }
        Ok(Nv { session, profile, set, get, save_fn: save, name_of, destroy })
    }
}

impl Nv {
    fn blank(id: u32) -> Box<Setting> {
        let mut s: Box<Setting> = Box::new(unsafe { std::mem::zeroed() });
        s.version = SETTING_VER;
        s.id = id;
        s
    }
    fn put(&self, id: u32, val: u32) -> Status {
        let mut s = Self::blank(id);
        s.ty = 0; // DWORD
        s.loc = 0; // current profile
        s.cur[0] = val;
        unsafe { (self.set)(self.session, self.profile, &mut *s) }
    }
    fn read(&self, id: u32) -> Result<u32, Status> {
        let mut s = Self::blank(id);
        let st = unsafe { (self.get)(self.session, self.profile, id, &mut *s) };
        if st == 0 { Ok(s.cur[0]) } else { Err(st) }
    }
    fn label(&self, id: u32) -> String {
        if let Some(f) = self.name_of {
            let mut buf = vec![0u16; 2048];
            if unsafe { f(id, buf.as_mut_ptr()) } == 0 {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
                return String::from_utf16_lossy(&buf[..n]);
            }
        }
        format!("0x{id:08X}")
    }
}

impl Drop for Nv {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.session) };
    }
}

/// (setting id, max-performance value, friendly name)
const MAX_PERF: &[(u32, u32, &str)] = &[
    (0x1057EB71, 1, "Power management mode = Prefer maximum performance"),
    (0x00A879CF, 0x08416747, "Vertical sync = Force off"),
    (0x007BA09E, 1, "Max pre-rendered frames = 1 (lowest latency)"),
    (0x00CE2691, 20, "Texture filtering quality = High performance"),
    (0x00E73211, 1, "Texture filtering anisotropic sample optimization = On"),
    (0x0019BB68, 1, "Texture filtering negative LOD bias = Allow"),
    (0x002ECAF2, 0, "Texture filtering trilinear optimization = On (driver value 0 = On, panel-verified)"),
    (0x20C1221E, 1, "Threaded optimization = On"),
    (0x10835002, 0, "Max frame rate limiter = Off (0)"),
    (0x00667329, 0, "Ambient occlusion = Off"),
    (0x1074C972, 0, "FXAA = Off"),
    (0x20FDD1F9, 0, "OpenGL triple buffering = Off"),
    (0x00198FFF, 1, "Shader cache = On"),
    (0x101E61A9, 16, "Anisotropic filtering level = 16x"),
    (0x10D2BB16, 1, "Anisotropic filtering mode = User defined"),
    (0x206A6582, 0, "OpenGL default swap interval = 0"),
    (0x10835016, 0, "Idle application max FPS limit = None"),
    (0x00AE785C, 0, "PowerThrottle = Off"),
    (0x10111133, 1, "VR pre-rendered frames = 1"),
    (0x0098C1AC, 0, "MFAA = Off"),
    (0x0064B541, 1, "Preferred refresh rate = Highest available"),
    (0x00738E8F, 0, "Texture filtering LOD bias = 0"),
    (0x10A879CF, 4, "G-SYNC application state = Fixed Refresh Rate"),
    (0x10A879AC, 4, "G-SYNC application requested state = Fixed Refresh Rate"),
    (0x1094F1F7, 0, "Monitor Technology: VRR requested state = Off (fixed refresh)"),
    (0x1094F157, 0, "Monitor Technology: VRR global feature = Off"),
    (0x1194F158, 0, "Monitor Technology: G-SYNC globally = Off"),
];

// ---------- helpers ----------
fn run(cmd: &str, args: &[&str]) -> (bool, String) {
    match Command::new(cmd).args(args).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s)
        }
        Err(e) => (false, e.to_string()),
    }
}

fn ps(script: &str) -> (bool, String) {
    run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])
}

fn is_admin() -> bool {
    run("net", &["session"]).0
}

fn nvcp_version() -> Option<String> {
    let (_, out) = ps("(Get-AppxPackage -AllUsers NVIDIACorp.NVIDIAControlPanel | Sort-Object Version -Descending | Select-Object -First 1).Version");
    let v = out.trim().to_string();
    if v.is_empty() || v.contains("rror") { None } else { Some(v) }
}

fn winget_path() -> String {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let p = format!("{base}\\Microsoft\\WindowsApps\\winget.exe");
    if std::path::Path::new(&p).exists() { p } else { "winget".into() }
}

// ---------- steps ----------
fn step_nvcp() -> (bool, String) {
    let w = winget_path();
    for i in 1..=ATTEMPTS {
        let before = nvcp_version();
        let args: Vec<&str> = if before.is_some() {
            vec!["upgrade", "--id", NVCP_STORE_ID, "--source", "msstore", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"]
        } else {
            vec!["install", "--id", NVCP_STORE_ID, "--source", "msstore", "--force", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"]
        };
        let (_, out) = run(&w, &args);
        if let Some(v) = nvcp_version() {
            return (true, format!("NVIDIA Control Panel {v}"));
        }
        eprintln!("nvcp attempt {i} failed: {}", out.lines().last().unwrap_or(""));
        sleep(Duration::from_secs(3));
    }
    (false, "NVIDIA Control Panel not installed".into())
}

fn step_nv() -> Vec<(bool, String)> {
    let mut rows = Vec::new();
    for attempt in 1..=ATTEMPTS {
        rows.clear();
        let nv = match nv_open() {
            Ok(n) => n,
            Err(e) => {
                rows.push((false, format!("NVAPI open: {e}")));
                sleep(Duration::from_secs(2));
                continue;
            }
        };
        for &(id, val, what) in MAX_PERF {
            let st = nv.put(id, val);
            if st != 0 {
                rows.push((false, format!("{what}: set failed status {st} [{}]", nv.label(id))));
            }
        }
        let st = nv.save(); // after loop
        let _ = st;
        for &(id, val, what) in MAX_PERF {
            match nv.read(id) {
                Ok(v) if v == val => rows.push((true, format!("{what}: {v} [{}]", nv.label(id)))),
                Ok(v) => rows.push((false, format!("{what}: read {v} want {val}"))),
                Err(s) => rows.push((false, format!("{what}: read failed status {s}"))),
            }
        }
        if rows.iter().all(|r| r.0) {
            return rows;
        }
        eprintln!("nvapi attempt {attempt} had failures, retrying");
        sleep(Duration::from_secs(2));
    }
    rows
}

impl Nv {
    fn save(&self) -> Status {
        unsafe { (self.save_fn)(self.session) }
    }
}

fn step_windows() -> Vec<(bool, String)> {
    let mut rows = Vec::new();
    // Ultimate Performance plan: create if missing, activate.
    let ult = "e9a42b02-d5df-448d-aa00-03f14749eb61";
    let is_max = |s: &str| {
        let l = s.to_lowercase();
        l.contains("ultimate") || l.contains("max performance")
    };
    let (_, cur) = run("powercfg", &["/getactivescheme"]);
    if is_max(&cur) {
        rows.push((true, format!("Power plan: {}", cur.trim())));
        return windows_rest(rows);
    }
    let (_, list) = run("powercfg", &["/list"]);
    if !list.to_lowercase().contains("ultimate") {
        run("powercfg", &["/duplicatescheme", ult]);
    }
    let (_, list) = run("powercfg", &["/list"]);
    let guid = list
        .lines()
        .find(|l| l.to_lowercase().contains("ultimate"))
        .and_then(|l| l.split_whitespace().nth(3))
        .map(|s| s.to_string());
    if let Some(g) = guid {
        run("powercfg", &["/setactive", &g]);
    }
    let (_, active) = run("powercfg", &["/getactivescheme"]);
    rows.push((is_max(&active), format!("Power plan: {}", active.trim())));
    windows_rest(rows)
}

/// Force one power-plan value (AC) on the active plan and read it back.
fn plan_value(rows: &mut Vec<(bool, String)>, sub: &str, setting: &str, want: u32, what: &str) {
    run("powercfg", &["/setacvalueindex", "SCHEME_CURRENT", sub, setting, &want.to_string()]);
    run("powercfg", &["/setactive", "SCHEME_CURRENT"]);
    let (_, q) = run("powercfg", &["/query", "SCHEME_CURRENT", sub, setting]);
    let got = q
        .lines()
        .find(|l| l.contains("Current AC Power Setting Index"))
        .and_then(|l| l.rsplit("0x").next())
        .and_then(|h| u32::from_str_radix(h.trim(), 16).ok());
    rows.push((got == Some(want), format!("{what}: {got:?}")));
}

/// Set a REG_DWORD and read it back through `reg query`.
fn reg_dword(rows: &mut Vec<(bool, String)>, key: &str, name: &str, want: u32, what: &str) {
    run("reg", &["add", key, "/v", name, "/t", "REG_DWORD", "/d", &want.to_string(), "/f"]);
    let (_, q) = run("reg", &["query", key, "/v", name]);
    let got = q
        .lines()
        .find(|l| l.contains(name))
        .and_then(|l| l.rsplit("0x").next())
        .and_then(|h| u32::from_str_radix(h.trim(), 16).ok());
    rows.push((got == Some(want), format!("{what}: {got:?}")));
}

fn windows_rest(mut rows: Vec<(bool, String)>) -> Vec<(bool, String)> {
    reg_dword(&mut rows, r"HKCU\System\GameConfigStore", "GameDVR_Enabled", 0, "Game DVR background recording = Off");
    reg_dword(&mut rows, r"HKCU\System\GameConfigStore", "GameDVR_FSEBehaviorMode", 2, "Fullscreen optimizations = Off (FSE behavior mode 2)");
    reg_dword(&mut rows, r"HKCU\System\GameConfigStore", "GameDVR_HonorUserFSEBehaviorMode", 1, "Honor user fullscreen behavior = On");
    reg_dword(&mut rows, r"HKCU\System\GameConfigStore", "GameDVR_DXGIHonorFSEWindowsCompatible", 1, "DXGI honor FSE compatibility = On");
    reg_dword(&mut rows, r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0, "Game Bar app capture = Off");
    reg_dword(&mut rows, r"HKLM\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling", "PowerThrottlingOff", 1, "Windows power throttling = Off");
    reg_dword(&mut rows, r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile", "SystemResponsiveness", 0, "Multimedia system responsiveness = 0 (max foreground priority)");
    plan_value(&mut rows, "SUB_PROCESSOR", "PROCTHROTTLEMIN", 100, "Minimum processor state = 100%");
    plan_value(&mut rows, "SUB_PROCESSOR", "PROCTHROTTLEMAX", 100, "Maximum processor state = 100%");
    plan_value(&mut rows, "SUB_PCIEXPRESS", "ASPM", 0, "PCI Express link state power management = Off");
    // Hardware-accelerated GPU scheduling on, Game Mode on.
    run("reg", &["add", r"HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "/v", "HwSchMode", "/t", "REG_DWORD", "/d", "2", "/f"]);
    let (_, q) = run("reg", &["query", r"HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers", "/v", "HwSchMode"]);
    rows.push((q.contains("0x2"), "Hardware-accelerated GPU scheduling = On (HwSchMode=2)".into()));
    run("reg", &["add", r"HKCU\Software\Microsoft\GameBar", "/v", "AutoGameModeEnabled", "/t", "REG_DWORD", "/d", "1", "/f"]);
    let (_, q) = run("reg", &["query", r"HKCU\Software\Microsoft\GameBar", "/v", "AutoGameModeEnabled"]);
    rows.push((q.contains("0x1"), "Game Mode = On".into()));
    rows
}

/// `nvmax stored`: enumerate the settings actually stored in the base profile (DRS_EnumSettings),
/// including ids that are not in the "available" list.
fn stored() -> i32 {
    let nv = match nv_open() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let m = unsafe { LoadLibraryA(c"nvapi64.dll".as_ptr().cast()) };
    let qp = unsafe { GetProcAddress(m, c"nvapi_QueryInterface".as_ptr().cast()) };
    let q: QueryIface = unsafe { std::mem::transmute(qp) };
    type EnumFn = unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut u32, *mut Setting) -> Status;
    let Some(en) = (unsafe { iface::<EnumFn>(q, 0xCFD6983E) }) else {
        eprintln!("DRS_EnumSettings missing");
        return 1;
    };
    let mut idx = 0u32;
    loop {
        let mut one = Nv::blank(0);
        let mut cnt = 1u32;
        let st = unsafe { en(nv.session, nv.profile, idx, &mut cnt, &mut *one) };
        if st == -7 && idx < 400 { idx += 1; continue; }
        if st != 0 || cnt == 0 {
            if st != 0 && st != -160 && st != -157 {
                eprintln!("enum status {st} at {idx}");
            }
            break;
        }
        println!("0x{:08X}\t0x{:08X}\ttype {}\t{}", one.id, one.cur[0], one.ty, nv.label(one.id));
        idx += 1;
    }
    println!("stored {idx}");
    0
}

/// `nvmax dump`: print every driver setting id, name and current base-profile value.
fn dump() -> i32 {
    let nv = match nv_open() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let m = unsafe { LoadLibraryA(c"nvapi64.dll".as_ptr().cast()) };
    let qp = unsafe { GetProcAddress(m, c"nvapi_QueryInterface".as_ptr().cast()) };
    let q: QueryIface = unsafe { std::mem::transmute(qp) };
    let en: Option<unsafe extern "C" fn(*mut u32, *mut u32) -> Status> = unsafe { iface(q, 0xF020614A) };
    let Some(en) = en else {
        eprintln!("EnumAvailableSettingIds missing");
        return 1;
    };
    let mut ids = vec![0u32; 4096];
    let mut n = 4096u32;
    let st = unsafe { en(ids.as_mut_ptr(), &mut n) };
    if st != 0 {
        eprintln!("enum status {st}");
        return 1;
    }
    for &id in &ids[..n as usize] {
        let cur = match nv.read(id) {
            Ok(v) => format!("0x{v:08X}"),
            Err(_) => "-".into(),
        };
        println!("0x{id:08X}\t{cur}\t{}", nv.label(id));
    }
    println!("total {n}");
    0
}

fn main() {
    if matches!(std::env::args().nth(1).as_deref(), Some("help" | "-h" | "--help")) {
        println!("nvmax            install/update NVIDIA Control Panel, apply every max-performance setting, verify (needs admin; self-elevates)");
        println!("nvmax verify     read-only check of the NVIDIA driver settings, exit 1 on mismatch");
        println!("nvmax dump       list every NVIDIA driver setting id, value and name");
        println!("nvmax set ID VAL force one driver setting (testing aid)");
        exit(0);
    }
    if std::env::args().nth(1).as_deref() == Some("stored") {
        exit(stored());
    }
    if std::env::args().nth(1).as_deref() == Some("dump") {
        exit(dump());
    }
    // `nvmax verify`: read-only check of every max-performance value; exit 1 on any mismatch.
    if std::env::args().nth(1).as_deref() == Some("verify") {
        let nv = nv_open().unwrap_or_else(|e| {
            eprintln!("{e}");
            exit(1)
        });
        let mut bad = 0;
        for &(id, val, what) in MAX_PERF {
            match nv.read(id) {
                Ok(v) if v == val => println!("[OK] {what}"),
                other => {
                    bad += 1;
                    println!("[FAIL] {what}: {other:?} want {val}");
                }
            }
        }
        for (ok, what) in npi_rows() {
            if ok {
                println!("[OK] {what}");
            } else {
                bad += 1;
                println!("[FAIL] {what}");
            }
        }
        println!("verify: {} checked, {bad} failed", MAX_PERF.len() + NPI_SETTINGS.len());
        exit(if bad == 0 { 0 } else { 1 });
    }
    // `nvmax set <id> <value>` (hex or decimal): force one setting, used to test recovery.
    if std::env::args().nth(1).as_deref() == Some("set") {
        let num = |s: Option<String>| {
            s.and_then(|s| match s.strip_prefix("0x") {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => s.parse().ok(),
            })
        };
        let mut a = std::env::args();
        let (id, val) = (num(a.nth(2)), num(a.next()));
        let (Some(id), Some(val)) = (id, val) else {
            eprintln!("usage: nvmax set <id> <value>");
            exit(2);
        };
        let nv = nv_open().unwrap_or_else(|e| {
            eprintln!("{e}");
            exit(1)
        });
        let st = nv.put(id, val);
        let sv = nv.save();
        println!("set 0x{id:08X}={val} status {st} save {sv}");
        exit(if st == 0 && sv == 0 { 0 } else { 1 });
    }
    let admin = is_admin();
    if !admin && std::env::var_os("NVMAX_ELEVATED").is_none() {
        // Not elevated: relaunch self as administrator and pass its exit code on.
        if let Ok(exe) = std::env::current_exe() {
            let script = format!(
                "$env:NVMAX_ELEVATED='1'; $p = Start-Process -FilePath '{}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
                exe.display()
            );
            let code = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .status()
                .ok()
                .and_then(|s| s.code())
                .unwrap_or(1);
            exit(code);
        }
    }
    println!("nvmax: admin={admin}");
    let mut all: Vec<(bool, String)> = Vec::new();
    all.push(step_nvcp());

    all.extend(step_npi());
    all.extend(step_nv());
    all.extend(step_windows());
    println!("---- verification ----");
    let mut bad = 0;
    for (ok, msg) in &all {
        println!("[{}] {msg}", if *ok { "OK" } else { "FAIL" });
        if !ok {
            bad += 1;
        }
    }
    println!("---- {} checks, {} failed ----", all.len(), bad);
    exit(if bad == 0 { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_struct_matches_nvapi_layout() {
        assert_eq!(std::mem::size_of::<Setting>(), 12320);
        assert_eq!(SETTING_VER, 12320 | (1 << 16));
    }

    #[test]
    fn apply_list_has_no_duplicate_ids() {
        let mut ids: Vec<u32> = MAX_PERF.iter().map(|r| r.0).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(n, ids.len());
    }
}
