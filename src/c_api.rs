use crate::GraphLite;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|cell| {
        if let Ok(c_str) = CString::new(msg) {
            *cell.borrow_mut() = c_str;
        }
    });
}

/// 打开数据库（支持文件路径与 ":memory:" 纯内存模式）
/// 成功返回 0，失败返回 -1
///
/// # Safety
///
/// 调用方必须确保 `path` 为以 null 结尾的有效 C 字符串指针，且 `out_db` 指向有效的可写指针内存。
/// 把单元格渲染为 JSON 文本。
///
/// 单元值本身可能是**已经构造好的 JSON 文本**（Cypher 的 `RETURN n` 会把节点
/// 渲染成 `{"k":v}` 形式，见 `render_binding`）。判断方式：以 `{` 或 `[` 开头
/// 且能被本模块编码器接受的，按原文嵌入；其余按普通字符串转义。
fn render_cell(v: &crate::graph::Value) -> String {
    match v {
        crate::graph::Value::String(s)
            if (s.starts_with('{') && s.ends_with('}'))
                || (s.starts_with('[') && s.ends_with(']')) =>
        {
            s.clone()
        }
        other => {
            let mut out = String::new();
            crate::json::write_value(&mut out, other);
            out
        }
    }
}

/// # Safety
///
/// `path` must be a valid NUL-terminated C string for the duration of the call.
/// `out_db` must point to writable memory able to hold a `*mut GraphLite`.
/// Both may be null, in which case the call fails with -1 rather than
/// dereferencing them.
///
/// On success the caller owns the returned handle and must release it with
/// `graphlite_close`; leaking it leaks the file lock and the buffer pool.
#[no_mangle]
pub unsafe extern "C" fn graphlite_open(path: *const c_char, out_db: *mut *mut GraphLite) -> c_int {
    if path.is_null() || out_db.is_null() {
        set_last_error("Null pointer provided to graphlite_open");
        return -1;
    }
    let c_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("Invalid UTF-8 path string: {}", e));
            return -1;
        }
    };

    match GraphLite::open(c_str) {
        Ok(db) => {
            *out_db = Box::into_raw(Box::new(db));
            0
        }
        Err(e) => {
            set_last_error(&e.to_string());
            -1
        }
    }
}

/// 关闭数据库句柄并释放内存资源
/// 成功返回 0，失败返回 -1
///
/// # Safety
///
/// 调用方必须确保 `db` 是由 `graphlite_open` 成功返回的有效句柄，且未被重复释放。
/// # Safety
///
/// `db` must be a handle previously returned by `graphlite_open` and not yet
/// closed. Passing any other pointer (including one already closed, or null) is
/// undefined behaviour. After this call the pointer is dangling and must not be
/// used again.
#[no_mangle]
pub unsafe extern "C" fn graphlite_close(db: *mut GraphLite) -> c_int {
    if db.is_null() {
        set_last_error("Null pointer provided to graphlite_close");
        return -1;
    }
    drop(Box::from_raw(db));
    0
}

/// 执行 Cypher 语句（查询或变更），结果序列化为 JSON 字符串存储于 out_json_result
/// 成功返回 0，失败返回 -1
///
/// # Safety
///
/// 调用方必须确保 `db` 为有效的打开句柄，`cypher` 为以 null 结尾的有效 C 字符串，
/// `out_json_result` 指向有效的指针内存。成功时返回的字符串必须使用 `graphlite_free_string` 释放。
/// # Safety
///
/// `db` must be a live handle from `graphlite_open`. `cypher` must be a valid
/// NUL-terminated C string. `out_json_result` must point to writable memory able
/// to hold a `*mut c_char`.
///
/// On success the caller owns the returned string and must free it with
/// `graphlite_free_string`. Null pointers produce -1 rather than a dereference.
#[no_mangle]
pub unsafe extern "C" fn graphlite_execute(
    db: *mut GraphLite,
    cypher: *const c_char,
    out_json_result: *mut *mut c_char,
) -> c_int {
    if db.is_null() || cypher.is_null() || out_json_result.is_null() {
        set_last_error("Null pointer provided to graphlite_execute");
        return -1;
    }
    let c_str = match CStr::from_ptr(cypher).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("Invalid UTF-8 cypher string: {}", e));
            return -1;
        }
    };

    let graph = &*db;
    match graph.query_cypher(c_str) {
        Ok(res) => {
            // 手写编码不返回错误：所有输入都能表示（非法浮点退化为 null）
            let rows: Vec<Vec<String>> = res
                .rows
                .iter()
                .map(|r| r.values.iter().map(render_cell).collect())
                .collect();
            let json = crate::json::result_set_to_string(&res.columns, &rows);
            let c_json = match CString::new(json) {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(&format!("CString creation error: {}", e));
                    return -1;
                }
            };
            *out_json_result = c_json.into_raw();
            0
        }
        Err(e) => {
            set_last_error(&e.to_string());
            -1
        }
    }
}

/// 释放由 graphlite_execute 分配的 C 字符串内存
///
/// # Safety
///
/// 调用方必须确保 `s` 是由 `graphlite_execute` 分配的有效 C 字符串指针，且未被重复释放。
/// # Safety
///
/// `s` must be a string previously returned by `graphlite_execute` (or null).
/// Freeing anything else, or freeing the same string twice, is undefined
/// behaviour.
#[no_mangle]
pub unsafe extern "C" fn graphlite_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// 获取最后一次操作失败的详细错误描述信息
///
/// # Safety
///
/// 返回的指针为当前线程局部生命周期内的 C 字符串，调用方不得释放此指针。
/// # Safety
///
/// The returned pointer refers to thread-local storage owned by this library. It
/// stays valid until the next call that sets an error **on the same thread**, so
/// copy it if you need to keep it. Do not free it.
#[no_mangle]
pub unsafe extern "C" fn graphlite_errmsg(_db: *mut GraphLite) -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}
