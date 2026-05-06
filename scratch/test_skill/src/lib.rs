use serde_json::{json, Value};

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn call(ptr: *mut u8, len: usize) -> u64 {
    let input_bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    let input_str = String::from_utf8_lossy(&input_bytes);
    let args: Value = serde_json::from_str(&input_str).unwrap_or(json!({}));

    let result = if let Some(input) = args["input"].as_str() {
        format!("WASM received: {}", input)
    } else {
        "No input provided to WASM".to_string()
    };

    let result_bytes = result.into_bytes();
    let result_len = result_bytes.len();
    let result_ptr = result_bytes.as_ptr();
    std::mem::forget(result_bytes);

    ((result_ptr as u64) << 32) | (result_len as u64)
}
