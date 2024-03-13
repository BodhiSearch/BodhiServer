use std::os::raw::{c_char, c_int, c_void};

extern "C" {
  pub fn llama_log_set(
    callback: extern "C" fn(c_int, *const c_char, *mut c_void),
    user_data: *mut c_void,
  );
}

#[allow(unused_variables)]
pub extern "C" fn null_log_callback(level: c_int, message: *const c_char, user_data: *mut c_void) {}
