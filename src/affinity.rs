use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::api;
use crate::config;
use crate::dx_overlay;

static ENABLED: AtomicBool = AtomicBool::new(true);
static TOGGLE_VK: AtomicU32 = AtomicU32::new(0x50);
static TOGGLE_NAME: Mutex<String> = Mutex::new(String::new());
static LABEL_TOTAL: Mutex<String> = Mutex::new(String::new());
static LABEL_P1: Mutex<String> = Mutex::new(String::new());
static LABEL_P2: Mutex<String> = Mutex::new(String::new());
static TOGGLE_WAS_DOWN: AtomicBool = AtomicBool::new(false);

static STEP: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicI32 = AtomicI32::new(-1);
static IND1: AtomicI32 = AtomicI32::new(-1);
static IND2: AtomicI32 = AtomicI32::new(-1);
static VAL_TS: AtomicU64 = AtomicU64::new(0);

static POS_X: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static POS_Y: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static SIZE: AtomicU32 = AtomicU32::new(0);

static CALC_TRAMP: AtomicUsize = AtomicUsize::new(0);
static SHOW_TRAMP: AtomicUsize = AtomicUsize::new(0);
static HIDE_TRAMP: AtomicUsize = AtomicUsize::new(0);
static ISDLG_TRAMP: AtomicUsize = AtomicUsize::new(0);

static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);
static LAST_TRAINEE: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_P1: AtomicUsize = AtomicUsize::new(usize::MAX);
static LAST_P2: AtomicUsize = AtomicUsize::new(usize::MAX);

const DEFAULT_POS: [[f32; 2]; 3] = [[0.35, 0.15], [0.15, 0.50], [0.30, 0.50]];
const DEFAULT_SIZE: f32 = 1.40;

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    config::save();
}

pub fn toggle_key_name() -> String {
    TOGGLE_NAME
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "P".into())
}

pub fn poll_toggle_hotkey() {
    let vk = TOGGLE_VK.load(Ordering::Relaxed);
    if vk == 0 {
        TOGGLE_WAS_DOWN.store(false, Ordering::Relaxed);
        return;
    }
    let down = key_down(vk as i32);
    if !game_focused() {
        TOGGLE_WAS_DOWN.store(down, Ordering::Relaxed);
        return;
    }
    let was = TOGGLE_WAS_DOWN.swap(down, Ordering::Relaxed);
    if down && !was {
        let next = !is_enabled();
        set_enabled(next);
        api::log_info(&format!(
            "AffNumber: overlay {} (hotkey {})",
            if next { "enabled" } else { "disabled" },
            toggle_key_name()
        ));
    }
}

fn clear_cached_values() {
    TOTAL.store(-1, Ordering::Relaxed);
    IND1.store(-1, Ordering::Relaxed);
    IND2.store(-1, Ordering::Relaxed);
    VAL_TS.store(0, Ordering::Relaxed);
    LAST_P1.store(usize::MAX, Ordering::Relaxed);
    LAST_P2.store(usize::MAX, Ordering::Relaxed);
}

pub fn on_legacy_select() -> bool {
    STEP.load(Ordering::Relaxed) != 0
}

pub fn should_draw_badges() -> bool {
    if !is_enabled() || !on_legacy_select() || DIALOG_OPEN.load(Ordering::Relaxed) {
        return false;
    }
    let Some((_, ind1, ind2)) = values() else {
        return false;
    };
    ind1 >= 0 || ind2 >= 0
}

pub fn dialog_open() -> bool {
    DIALOG_OPEN.load(Ordering::Relaxed)
}

fn label_get(m: &Mutex<String>, fallback: &str) -> String {
    m.lock()
        .map(|g| {
            let t = g.trim();
            if t.is_empty() {
                fallback.into()
            } else {
                t.to_string()
            }
        })
        .unwrap_or_else(|_| fallback.into())
}

pub fn label_total() -> String {
    label_get(&LABEL_TOTAL, "Total")
}
pub fn label_parent1() -> String {
    label_get(&LABEL_P1, "Parent 1")
}
pub fn label_parent2() -> String {
    label_get(&LABEL_P2, "Parent 2")
}

pub fn badge_items() -> Vec<(usize, String, i32)> {
    let Some((total, ind1, ind2)) = values() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(3);
    if total >= 0 {
        out.push((0, label_total(), total));
    }
    if ind1 >= 0 {
        out.push((1, label_parent1(), ind1));
    }
    if ind2 >= 0 {
        out.push((2, label_parent2(), ind2));
    }
    out
}

fn key_down(vk: i32) -> bool {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) < 0 }
}

fn game_focused() -> bool {
    unsafe {
        use windows::Win32::System::Threading::GetCurrentProcessId;
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

fn parent_ptr_set(p: usize) -> bool {
    p != 0 && p != usize::MAX
}

pub fn values() -> Option<(i32, i32, i32)> {
    if VAL_TS.load(Ordering::Relaxed) == 0 {
        return None;
    }
    Some((
        TOTAL.load(Ordering::Relaxed),
        IND1.load(Ordering::Relaxed),
        IND2.load(Ordering::Relaxed),
    ))
}

fn bits(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}
fn set_bits(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

pub fn pos(i: usize) -> (f32, f32) {
    (bits(&POS_X[i]), bits(&POS_Y[i]))
}
fn set_pos(i: usize, fx: f32, fy: f32) {
    set_bits(&POS_X[i], fx.clamp(0.0, 1.0));
    set_bits(&POS_Y[i], fy.clamp(0.0, 1.0));
}
pub fn size() -> f32 {
    bits(&SIZE)
}
pub fn set_size(s: f32) {
    set_bits(&SIZE, s.clamp(0.8, 4.0));
    config::save();
}

pub fn reset_positions() {
    apply_default_layout();
    config::save();
}

fn apply_default_layout() {
    set_bits(&SIZE, DEFAULT_SIZE);
    for (i, [x, y]) in DEFAULT_POS.iter().enumerate() {
        set_pos(i, *x, *y);
    }
}

pub fn load_defaults_and_cfg() {
    apply_default_layout();
    store_str(&TOGGLE_NAME, "P");
    store_str(&LABEL_TOTAL, "Total");
    store_str(&LABEL_P1, "Parent 1");
    store_str(&LABEL_P2, "Parent 2");
    TOGGLE_VK.store(0x50, Ordering::Relaxed);
    config::load();
}

fn store_str(m: &Mutex<String>, s: &str) {
    if let Ok(mut g) = m.lock() {
        *g = s.into();
    }
}

pub fn apply_cfg(
    enabled: bool,
    toggle_key: &str,
    size: f32,
    total: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    label_total: &str,
    label_p1: &str,
    label_p2: &str,
) {
    ENABLED.store(enabled, Ordering::Relaxed);
    let normalized = config::normalize_name(toggle_key);
    TOGGLE_VK.store(config::parse_vk(&normalized).unwrap_or(0), Ordering::Relaxed);
    store_str(&TOGGLE_NAME, &normalized);
    set_bits(&SIZE, size.clamp(0.8, 4.0));
    set_pos(0, total[0], total[1]);
    set_pos(1, p1[0], p1[1]);
    set_pos(2, p2[0], p2[1]);
    store_str(&LABEL_TOTAL, &sanitize_label(label_total, "Total"));
    store_str(&LABEL_P1, &sanitize_label(label_p1, "Parent 1"));
    store_str(&LABEL_P2, &sanitize_label(label_p2, "Parent 2"));
}

fn sanitize_label(s: &str, fallback: &str) -> String {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(24)
        .collect();
    if t.is_empty() {
        fallback.into()
    } else {
        t
    }
}

pub fn snapshot_cfg() -> (bool, String, f32, [f32; 2], [f32; 2], [f32; 2], String, String, String)
{
    (
        is_enabled(),
        toggle_key_name(),
        size(),
        [pos(0).0, pos(0).1],
        [pos(1).0, pos(1).1],
        [pos(2).0, pos(2).1],
        label_total(),
        label_parent1(),
        label_parent2(),
    )
}

type CalcFn = unsafe extern "C" fn(i32, usize, usize, usize) -> i32;

#[derive(Clone, Copy)]
enum ParentSlot {
    One,
    Two,
}

fn slot_of(p: usize, lp1: usize, lp2: usize) -> Option<ParentSlot> {
    if parent_ptr_set(p) && p == lp1 {
        Some(ParentSlot::One)
    } else if parent_ptr_set(p) && p == lp2 {
        Some(ParentSlot::Two)
    } else {
        None
    }
}

fn store_values(total: i32, ind1: i32, ind2: i32, p1: usize, p2: usize, trainee: i32) {
    TOTAL.store(total, Ordering::Relaxed);
    IND1.store(ind1, Ordering::Relaxed);
    IND2.store(ind2, Ordering::Relaxed);
    VAL_TS.store(1, Ordering::Relaxed);
    LAST_TRAINEE.store(trainee, Ordering::Relaxed);
    LAST_P1.store(if parent_ptr_set(p1) { p1 } else { usize::MAX }, Ordering::Relaxed);
    LAST_P2.store(if parent_ptr_set(p2) { p2 } else { usize::MAX }, Ordering::Relaxed);
}

fn call_order(p1: usize, p2: usize, va: i32, vb: i32) -> (i32, i32, usize, usize) {
    (
        if parent_ptr_set(p1) { va } else { -1 },
        if parent_ptr_set(p2) { vb } else { -1 },
        if parent_ptr_set(p1) { p1 } else { usize::MAX },
        if parent_ptr_set(p2) { p2 } else { usize::MAX },
    )
}

fn set_slot(
    slot: ParentSlot,
    val: i32,
    ptr: usize,
    s1: &mut i32,
    s2: &mut i32,
    op1: &mut usize,
    op2: &mut usize,
) {
    match slot {
        ParentSlot::One => {
            *s1 = val;
            *op1 = ptr;
        }
        ParentSlot::Two => {
            *s2 = val;
            *op2 = ptr;
        }
    }
}

fn place_new(
    ptr: usize,
    val: i32,
    other: Option<ParentSlot>,
    s1: &mut i32,
    s2: &mut i32,
    op1: &mut usize,
    op2: &mut usize,
) {
    match other {
        Some(ParentSlot::One) => set_slot(ParentSlot::Two, val, ptr, s1, s2, op1, op2),
        Some(ParentSlot::Two) => set_slot(ParentSlot::One, val, ptr, s1, s2, op1, op2),
        None if !parent_ptr_set(*op1) => set_slot(ParentSlot::One, val, ptr, s1, s2, op1, op2),
        None if !parent_ptr_set(*op2) => set_slot(ParentSlot::Two, val, ptr, s1, s2, op1, op2),
        None => {}
    }
}

fn assign_by_identity(
    p1: usize,
    p2: usize,
    va: i32,
    vb: i32,
    lp1: usize,
    lp2: usize,
) -> (i32, i32, usize, usize) {
    if !parent_ptr_set(lp1) && !parent_ptr_set(lp2) {
        return call_order(p1, p2, va, vb);
    }

    let a_slot = slot_of(p1, lp1, lp2);
    let b_slot = slot_of(p2, lp1, lp2);
    let a_new = parent_ptr_set(p1) && a_slot.is_none();
    let b_new = parent_ptr_set(p2) && b_slot.is_none();
    if (a_new && b_new) || (a_new && !parent_ptr_set(p2)) || (b_new && !parent_ptr_set(p1)) {
        return call_order(p1, p2, va, vb);
    }

    let mut s1 = -1i32;
    let mut s2 = -1i32;
    let mut op1 = usize::MAX;
    let mut op2 = usize::MAX;
    if let Some(s) = a_slot {
        set_slot(s, va, p1, &mut s1, &mut s2, &mut op1, &mut op2);
    }
    if let Some(s) = b_slot {
        set_slot(s, vb, p2, &mut s1, &mut s2, &mut op1, &mut op2);
    }
    if a_new {
        place_new(p1, va, b_slot, &mut s1, &mut s2, &mut op1, &mut op2);
    }
    if b_new {
        place_new(p2, vb, a_slot, &mut s1, &mut s2, &mut op1, &mut op2);
    }
    (s1, s2, op1, op2)
}

unsafe extern "C" fn calc_hook(trainee: i32, p1: usize, p2: usize, mi: usize) -> i32 {
    let tr = CALC_TRAMP.load(Ordering::Relaxed);
    if tr == 0 {
        return 0;
    }
    let f: CalcFn = std::mem::transmute(tr);
    let total = f(trainee, p1, p2, mi);

    if STEP.load(Ordering::Relaxed) == 0 {
        return total;
    }

    let lp1 = LAST_P1.load(Ordering::Relaxed);
    let lp2 = LAST_P2.load(Ordering::Relaxed);
    if LAST_TRAINEE.load(Ordering::Relaxed) == trainee
        && lp1 == p1
        && lp2 == p2
        && VAL_TS.load(Ordering::Relaxed) != 0
        && TOTAL.load(Ordering::Relaxed) == total
    {
        return total;
    }

    if !parent_ptr_set(p1) && !parent_ptr_set(p2) {
        clear_cached_values();
        LAST_TRAINEE.store(trainee, Ordering::Relaxed);
        return total;
    }

    if !(0..=800).contains(&total) {
        return total;
    }

    let va = if parent_ptr_set(p1) {
        f(trainee, p1, 0, mi)
    } else {
        -1
    };
    let vb = if parent_ptr_set(p2) {
        f(trainee, p2, 0, mi)
    } else {
        -1
    };

    let (ind1, ind2, op1, op2) = assign_by_identity(p1, p2, va, vb, lp1, lp2);
    store_values(total, ind1, ind2, op1, op2, trainee);
    total
}

type ShowFn = unsafe extern "C" fn(*mut c_void, *const c_void);
unsafe extern "C" fn show_hook(this: *mut c_void, mi: *const c_void) {
    if !this.is_null() {
        STEP.store(this as usize, Ordering::Relaxed);
    }
    clear_cached_values();
    let o = SHOW_TRAMP.load(Ordering::Relaxed);
    if o != 0 {
        let f: ShowFn = std::mem::transmute(o);
        f(this, mi);
    }
}

type HideFn = unsafe extern "C" fn(*mut c_void, bool, *const c_void);
unsafe extern "C" fn hide_hook(this: *mut c_void, force: bool, mi: *const c_void) {
    STEP.store(0, Ordering::Relaxed);
    clear_cached_values();
    DIALOG_OPEN.store(false, Ordering::Relaxed);
    LAST_TRAINEE.store(i32::MIN, Ordering::Relaxed);
    let o = HIDE_TRAMP.load(Ordering::Relaxed);
    if o != 0 {
        let f: HideFn = std::mem::transmute(o);
        f(this, force, mi);
    }
}

type IsDlgFn = unsafe extern "C" fn(*const c_void) -> bool;
unsafe extern "C" fn isdlg_hook(mi: *const c_void) -> bool {
    let tr = ISDLG_TRAMP.load(Ordering::Relaxed);
    if tr == 0 {
        return false;
    }
    let f: IsDlgFn = std::mem::transmute(tr);
    let open = f(mi);
    DIALOG_OPEN.store(open, Ordering::Relaxed);
    open
}

fn hook_method(
    class: *mut c_void,
    name: &str,
    argc: i32,
    detour: *mut c_void,
) -> Option<*mut c_void> {
    let m = api::get_method_addr(class, name, argc);
    if m.is_null() {
        return None;
    }
    let tramp = api::hook(m, detour);
    if tramp.is_null() {
        None
    } else {
        Some(tramp)
    }
}

pub fn install() -> String {
    load_defaults_and_cfg();
    dx_overlay::start();
    let mut notes = String::new();

    let image = api::game_image();
    if image.is_null() {
        return "game assembly not found".into();
    }

    let smu = api::get_class(image, "Gallop", "SingleModeUtils");
    if smu.is_null() {
        return "SingleModeUtils not found".into();
    }
    match hook_method(smu, "CalcRelationPoint", 3, calc_hook as *mut c_void) {
        Some(tramp) => {
            CALC_TRAMP.store(tramp as usize, Ordering::Relaxed);
            notes.push_str("calc:ok ");
        }
        None => notes.push_str("calc:miss "),
    }

    let step = api::get_class(image, "Gallop", "SingleModeStartStepSuccessionSelect");
    if step.is_null() {
        notes.push_str("step:miss ");
    } else {
        match hook_method(step, "Show", 0, show_hook as *mut c_void) {
            Some(tramp) => {
                SHOW_TRAMP.store(tramp as usize, Ordering::Relaxed);
                notes.push_str("show:ok ");
            }
            None => notes.push_str("show:fail "),
        }
        match hook_method(step, "Hide", 1, hide_hook as *mut c_void) {
            Some(tramp) => {
                HIDE_TRAMP.store(tramp as usize, Ordering::Relaxed);
                notes.push_str("hide:ok ");
            }
            None => notes.push_str("hide:fail "),
        }
    }

    let dm = api::get_class(image, "Gallop", "DialogManager");
    if dm.is_null() {
        notes.push_str("dialog:miss");
    } else {
        match hook_method(dm, "get_IsShowDialog", 0, isdlg_hook as *mut c_void) {
            Some(tramp) => {
                ISDLG_TRAMP.store(tramp as usize, Ordering::Relaxed);
                notes.push_str("dialog:ok");
            }
            None => notes.push_str("dialog:miss"),
        }
    }

    format!("affinity: {}", notes.trim())
}
