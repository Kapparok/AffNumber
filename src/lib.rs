mod affinity;
mod api;
mod config;
mod dx_overlay;

use std::ffi::c_void;

use api::{HachimiGetApiFn, InitResult};

#[no_mangle]
pub unsafe extern "C" fn hachimi_init_v3(get_api: HachimiGetApiFn, version: i32) -> InitResult {
    if version < 3 {
        return InitResult::Error;
    }
    if !api::init(get_api) {
        return InitResult::Error;
    }

    api::log_info("AffNumber 1.0.1 loading…");

    if !api::register_menu_section("AffNumber", config::menu_section) {
        api::log_warn("failed to register menu section");
    }

    let notes = affinity::install();
    api::log_info(&notes);

    InitResult::Ok
}

#[no_mangle]
pub unsafe extern "C" fn hachimi_init(_vtable: *const c_void, _version: i32) -> InitResult {
    InitResult::Error
}
