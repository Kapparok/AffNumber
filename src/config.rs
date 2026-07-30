use std::ffi::c_void;

use serde::{Deserialize, Serialize};

use crate::affinity;
use crate::api;
use crate::dx_overlay;

#[derive(Clone, Copy, Serialize, Deserialize)]
struct ScreenPos {
    x: f32,
    y: f32,
}

impl ScreenPos {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    fn as_arr(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

#[derive(Serialize, Deserialize)]
struct BadgePositions {
    total: ScreenPos,
    parent_1: ScreenPos,
    parent_2: ScreenPos,
}

#[derive(Serialize, Deserialize)]
struct BadgeLabels {
    #[serde(default = "default_label_total")]
    total: String,
    #[serde(default = "default_label_p1")]
    parent_1: String,
    #[serde(default = "default_label_p2")]
    parent_2: String,
}

fn default_label_total() -> String {
    "Total".into()
}
fn default_label_p1() -> String {
    "Parent 1".into()
}
fn default_label_p2() -> String {
    "Parent 2".into()
}

impl Default for BadgeLabels {
    fn default() -> Self {
        Self {
            total: default_label_total(),
            parent_1: default_label_p1(),
            parent_2: default_label_p2(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Cfg {
    enabled: bool,
    #[serde(default = "default_toggle_key")]
    toggle_key: String,
    badge_size: f32,
    positions: BadgePositions,
    #[serde(default)]
    labels: BadgeLabels,
}

fn default_toggle_key() -> String {
    "P".into()
}

impl Cfg {
    fn from_runtime(
        enabled: bool,
        toggle_key: String,
        size: f32,
        total: [f32; 2],
        p1: [f32; 2],
        p2: [f32; 2],
        label_total: String,
        label_p1: String,
        label_p2: String,
    ) -> Self {
        Self {
            enabled,
            toggle_key: normalize_name(&toggle_key),
            badge_size: size,
            positions: BadgePositions {
                total: ScreenPos::new(total[0], total[1]),
                parent_1: ScreenPos::new(p1[0], p1[1]),
                parent_2: ScreenPos::new(p2[0], p2[1]),
            },
            labels: BadgeLabels {
                total: label_total,
                parent_1: label_p1,
                parent_2: label_p2,
            },
        }
    }

    fn apply(&self) {
        affinity::apply_cfg(
            self.enabled,
            &self.toggle_key,
            self.badge_size,
            self.positions.total.as_arr(),
            self.positions.parent_1.as_arr(),
            self.positions.parent_2.as_arr(),
            &self.labels.total,
            &self.labels.parent_1,
            &self.labels.parent_2,
        );
    }
}

#[derive(Deserialize)]
struct CfgLegacy {
    enabled: bool,
    size: f32,
    total: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
}

impl From<CfgLegacy> for Cfg {
    fn from(old: CfgLegacy) -> Self {
        Cfg::from_runtime(
            old.enabled,
            default_toggle_key(),
            old.size,
            old.total,
            old.p1,
            old.p2,
            default_label_total(),
            default_label_p1(),
            default_label_p2(),
        )
    }
}

fn path() -> Option<std::path::PathBuf> {
    let dir = api::base_dir()
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("hachimi")))
        })
        .or_else(|| std::env::current_dir().ok().map(|d| d.join("hachimi")))?;
    Some(dir.join("affnumber.json"))
}

pub fn config_path_display() -> Option<String> {
    path().map(|p| p.display().to_string())
}

fn parse_cfg(bytes: &[u8]) -> Result<(Cfg, bool), String> {
    if let Ok(cfg) = serde_json::from_slice::<Cfg>(bytes) {
        return Ok((cfg, false));
    }
    match serde_json::from_slice::<CfgLegacy>(bytes) {
        Ok(old) => Ok((Cfg::from(old), true)),
        Err(e) => Err(e.to_string()),
    }
}

pub fn load() {
    let Some(path) = path() else {
        api::log_warn("AffNumber: could not resolve config path");
        return;
    };
    if !path.exists() {
        api::log_info(&format!(
            "AffNumber: no config at {}; writing defaults",
            path.display()
        ));
        save();
        return;
    }
    let Ok(bytes) = std::fs::read(&path) else {
        api::log_warn(&format!("AffNumber: failed to read {}", path.display()));
        return;
    };
    match parse_cfg(&bytes) {
        Ok((cfg, migrated)) => {
            cfg.apply();
            api::log_info(&format!("AffNumber: config loaded from {}", path.display()));
            if migrated {
                save();
            }
        }
        Err(e) => {
            api::log_warn(&format!(
                "AffNumber: config parse failed ({}): {e}",
                path.display()
            ));
        }
    }
}

pub fn save() {
    let Some(path) = path() else {
        api::log_warn("AffNumber: could not resolve config path (save skipped)");
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            api::log_warn(&format!(
                "AffNumber: create_dir_all {}: {e}",
                parent.display()
            ));
            return;
        }
    }
    let (enabled, toggle_key, size, total, p1, p2, lt, lp1, lp2) = affinity::snapshot_cfg();
    let cfg = Cfg::from_runtime(enabled, toggle_key, size, total, p1, p2, lt, lp1, lp2);
    match serde_json::to_string_pretty(&cfg) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                api::log_warn(&format!(
                    "AffNumber: config write failed ({}): {e}",
                    path.display()
                ));
            }
        }
        Err(e) => api::log_warn(&format!("AffNumber: config serialize failed: {e}")),
    }
}

pub fn parse_vk(name: &str) -> Option<u32> {
    let s = name.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("off") {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + (n - 1));
            }
        }
    }
    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
    }
    match upper.as_str() {
        "INSERT" | "INS" => Some(0x2D),
        "DELETE" | "DEL" => Some(0x2E),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" | "PGUP" => Some(0x21),
        "PAGEDOWN" | "PGDN" => Some(0x22),
        _ => None,
    }
}

pub fn normalize_name(name: &str) -> String {
    let s = name.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("off") {
        return "None".into();
    }
    let upper = s.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix('F') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return format!("F{n}");
            }
        }
    }
    if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() {
        return upper;
    }
    match upper.as_str() {
        "INS" => "Insert".into(),
        "DEL" => "Delete".into(),
        "PGUP" => "PageUp".into(),
        "PGDN" => "PageDown".into(),
        other => {
            let mut out = String::with_capacity(other.len());
            let mut first = true;
            for ch in other.chars() {
                if first {
                    out.extend(ch.to_uppercase());
                    first = false;
                } else {
                    out.extend(ch.to_lowercase());
                }
            }
            out
        }
    }
}

pub extern "C" fn menu_section(ui: *mut c_void, _userdata: *mut c_void) {
    api::ui_heading(ui, "AffNumber");
    api::ui_small(
        ui,
        "Legacy Select affinity (CalcRelationPoint). Drawn via DXGI Present.",
    );
    api::ui_separator(ui);

    let status = if !affinity::is_enabled() {
        format!(
            "Overlay disabled — press {} or enable below",
            affinity::toggle_key_name()
        )
    } else if !dx_overlay::present_ok() {
        "Present not ready — update Hachimi Edge / try windowed mode".into()
    } else if affinity::should_draw_badges() {
        "Legacy Select open — badges drawing".into()
    } else if affinity::on_legacy_select() && affinity::dialog_open() {
        "Legacy Select — badges hidden (detail dialog)".into()
    } else if affinity::on_legacy_select() {
        "Legacy Select — waiting for affinity calc".into()
    } else {
        "Open Legacy Select to capture values".into()
    };
    let (r, g, b) = if !affinity::is_enabled() {
        (220, 120, 100)
    } else if affinity::should_draw_badges() {
        (90, 220, 120)
    } else {
        (220, 180, 80)
    };
    api::ui_colored_label(ui, r, g, b, 255, &status);
    let notes = affinity::install_notes();
    if !notes.is_empty() {
        api::ui_small(ui, &notes);
    }
    if let Some(err) = dx_overlay::last_draw_error() {
        api::ui_colored_label(ui, 220, 120, 100, 255, &format!("Draw: {err}"));
    }

    let mut en = affinity::is_enabled();
    if api::ui_checkbox(ui, "Show affinity numbers", &mut en) {
        affinity::set_enabled(en);
    }
    api::ui_small(
        ui,
        &format!(
            "Toggle hotkey: {} (edit toggle_key in affnumber.json)",
            affinity::toggle_key_name()
        ),
    );
    api::ui_label(ui, &format!("Badge size: {:.2}", affinity::size()));

    let mut bigger = false;
    let mut smaller = false;
    let mut reset = false;
    let mut reset_pos = false;
    api::ui_checkbox(ui, "Size +", &mut bigger);
    if bigger {
        affinity::set_size(affinity::size() + 0.15);
    }
    api::ui_checkbox(ui, "Size −", &mut smaller);
    if smaller {
        affinity::set_size(affinity::size() - 0.15);
    }
    api::ui_checkbox(ui, "Reset size", &mut reset);
    if reset {
        affinity::set_size(1.40);
    }
    api::ui_checkbox(ui, "Reset positions", &mut reset_pos);
    if reset_pos {
        affinity::reset_positions();
    }

    api::ui_separator(ui);
    if let Some((t, a, b)) = affinity::values() {
        let fmt = |v: i32| if v < 0 { "—".into() } else { v.to_string() };
        api::ui_colored_label(
            ui,
            230,
            230,
            235,
            255,
            &format!("{}  {t}", affinity::label_total()),
        );
        api::ui_colored_label(
            ui,
            180,
            200,
            220,
            255,
            &format!("{}  {}", affinity::label_parent1(), fmt(a)),
        );
        api::ui_colored_label(
            ui,
            180,
            200,
            220,
            255,
            &format!("{}  {}", affinity::label_parent2(), fmt(b)),
        );
    } else {
        api::ui_small(ui, "No affinity computed yet this screen.");
    }

    if let Some(path) = config_path_display() {
        api::ui_separator(ui);
        api::ui_small(ui, &format!("Config: {path}"));
    }
}
