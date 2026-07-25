use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

pub type HachimiGetApiFn = unsafe extern "C" fn(name: *const c_char) -> *mut c_void;

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InitResult {
    Error = 0,
    Ok = 1,
}

pub type GuiMenuSectionCallback = extern "C" fn(ui: *mut c_void, userdata: *mut c_void);

struct ApiFns {
    log: unsafe extern "C" fn(level: i32, target: *const c_char, message: *const c_char),
    hachimi_instance: unsafe extern "C" fn() -> *const c_void,
    hachimi_get_interceptor: unsafe extern "C" fn(*const c_void) -> *const c_void,
    interceptor_hook: unsafe extern "C" fn(
        *const c_void,
        *mut c_void,
        *mut c_void,
    ) -> *mut c_void,
    interceptor_get_trampoline_addr: unsafe extern "C" fn(*const c_void, *mut c_void) -> *mut c_void,
    il2cpp_get_assembly_image: unsafe extern "C" fn(*const c_char) -> *const c_void,
    il2cpp_get_class: unsafe extern "C" fn(
        *const c_void,
        *const c_char,
        *const c_char,
    ) -> *mut c_void,
    il2cpp_get_method_addr: unsafe extern "C" fn(*mut c_void, *const c_char, i32) -> *mut c_void,
    gui_register_menu_section_with_icon: unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const u8,
        usize,
        Option<GuiMenuSectionCallback>,
        *mut c_void,
    ) -> bool,
    gui_ui_heading: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    gui_ui_label: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    gui_ui_small: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    gui_ui_separator: unsafe extern "C" fn(*mut c_void) -> bool,
    gui_ui_checkbox: unsafe extern "C" fn(*mut c_void, *const c_char, *mut bool) -> bool,
    gui_ui_colored_label: unsafe extern "C" fn(*mut c_void, u8, u8, u8, u8, *const c_char) -> bool,
    hachimi_get_base_dir: unsafe extern "C" fn() -> *const c_char,
    hachimi_register_present_callback: unsafe extern "C" fn(
        Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        *mut c_void,
    ) -> bool,
}

static API: OnceLock<ApiFns> = OnceLock::new();

fn resolve(get_api: HachimiGetApiFn, name: &str) -> *mut c_void {
    let c = CString::new(name).unwrap();
    unsafe { get_api(c.as_ptr()) }
}

macro_rules! req {
    ($get:expr, $name:expr) => {{
        let p = resolve($get, $name);
        if p.is_null() {
            return false;
        }
        unsafe { std::mem::transmute(p) }
    }};
}

pub fn init(get_api: HachimiGetApiFn) -> bool {
    let fns = ApiFns {
        log: req!(get_api, "log"),
        hachimi_instance: req!(get_api, "hachimi_instance"),
        hachimi_get_interceptor: req!(get_api, "hachimi_get_interceptor"),
        interceptor_hook: req!(get_api, "interceptor_hook"),
        interceptor_get_trampoline_addr: req!(get_api, "interceptor_get_trampoline_addr"),
        il2cpp_get_assembly_image: req!(get_api, "il2cpp_get_assembly_image"),
        il2cpp_get_class: req!(get_api, "il2cpp_get_class"),
        il2cpp_get_method_addr: req!(get_api, "il2cpp_get_method_addr"),
        gui_register_menu_section_with_icon: req!(get_api, "gui_register_menu_section_with_icon"),
        gui_ui_heading: req!(get_api, "gui_ui_heading"),
        gui_ui_label: req!(get_api, "gui_ui_label"),
        gui_ui_small: req!(get_api, "gui_ui_small"),
        gui_ui_separator: req!(get_api, "gui_ui_separator"),
        gui_ui_checkbox: req!(get_api, "gui_ui_checkbox"),
        gui_ui_colored_label: req!(get_api, "gui_ui_colored_label"),
        hachimi_get_base_dir: req!(get_api, "hachimi_get_base_dir"),
        hachimi_register_present_callback: req!(get_api, "hachimi_register_present_callback"),
    };
    API.set(fns).is_ok()
}

fn api() -> &'static ApiFns {
    API.get().expect("api not initialized")
}

pub fn log_info(msg: &str) {
    log(3, msg);
}
pub fn log_warn(msg: &str) {
    log(2, msg);
}

fn log(level: i32, msg: &str) {
    let Ok(target) = CString::new("AffNumber") else {
        return;
    };
    let Ok(message) = CString::new(msg) else {
        return;
    };
    unsafe {
        (api().log)(level, target.as_ptr(), message.as_ptr());
    }
}

pub fn hook(orig: *mut c_void, detour: *mut c_void) -> *mut c_void {
    let a = api();
    unsafe {
        let hachimi = (a.hachimi_instance)();
        let interceptor = (a.hachimi_get_interceptor)(hachimi);
        let tramp = (a.interceptor_hook)(interceptor, orig, detour);
        if tramp.is_null() {
            return std::ptr::null_mut();
        }
        let t2 = (a.interceptor_get_trampoline_addr)(interceptor, detour);
        if !t2.is_null() {
            t2
        } else {
            tramp
        }
    }
}

pub fn get_assembly_image(name: &str) -> *const c_void {
    let a = api();
    let name = CString::new(name).unwrap();
    unsafe { (a.il2cpp_get_assembly_image)(name.as_ptr()) }
}

pub fn game_image() -> *const c_void {
    for name in ["umamusume.dll", "Assembly-CSharp.dll"] {
        let img = get_assembly_image(name);
        if !img.is_null() {
            log_info(&format!("game assembly: {name}"));
            return img;
        }
    }
    std::ptr::null()
}

pub fn get_class(image: *const c_void, ns: &str, name: &str) -> *mut c_void {
    let a = api();
    let ns = CString::new(ns).unwrap();
    let name = CString::new(name).unwrap();
    unsafe { (a.il2cpp_get_class)(image, ns.as_ptr(), name.as_ptr()) }
}

pub fn get_method_addr(class: *mut c_void, name: &str, argc: i32) -> *mut c_void {
    let a = api();
    let name = CString::new(name).unwrap();
    unsafe { (a.il2cpp_get_method_addr)(class, name.as_ptr(), argc) }
}

pub fn register_menu_section(title: &str, cb: GuiMenuSectionCallback) -> bool {
    let a = api();
    let title = CString::new(title).unwrap();
    unsafe {
        (a.gui_register_menu_section_with_icon)(
            title.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            Some(cb),
            std::ptr::null_mut(),
        )
    }
}

pub fn ui_heading(ui: *mut c_void, text: &str) {
    let t = CString::new(text).unwrap();
    unsafe {
        (api().gui_ui_heading)(ui, t.as_ptr());
    }
}
pub fn ui_label(ui: *mut c_void, text: &str) {
    let t = CString::new(text).unwrap();
    unsafe {
        (api().gui_ui_label)(ui, t.as_ptr());
    }
}
pub fn ui_small(ui: *mut c_void, text: &str) {
    let t = CString::new(text).unwrap();
    unsafe {
        (api().gui_ui_small)(ui, t.as_ptr());
    }
}
pub fn ui_separator(ui: *mut c_void) {
    unsafe {
        (api().gui_ui_separator)(ui);
    }
}
pub fn ui_checkbox(ui: *mut c_void, text: &str, value: &mut bool) -> bool {
    let t = CString::new(text).unwrap();
    unsafe { (api().gui_ui_checkbox)(ui, t.as_ptr(), value as *mut bool) }
}
pub fn ui_colored_label(ui: *mut c_void, r: u8, g: u8, b: u8, a: u8, text: &str) {
    let t = CString::new(text).unwrap();
    unsafe {
        (api().gui_ui_colored_label)(ui, r, g, b, a, t.as_ptr());
    }
}

pub fn base_dir() -> Option<std::path::PathBuf> {
    let p = unsafe { (api().hachimi_get_base_dir)() };
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    if s.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(s))
    }
}

pub fn register_present_callback(
    cb: unsafe extern "C" fn(swapchain: *mut c_void, userdata: *mut c_void),
) -> bool {
    unsafe { (api().hachimi_register_present_callback)(Some(cb), std::ptr::null_mut()) }
}
