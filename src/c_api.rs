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
            let json = match serde_json::to_string(&res) {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(&format!("JSON serialization error: {}", e));
                    return -1;
                }
            };
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
#[no_mangle]
pub unsafe extern "C" fn graphlite_errmsg(_db: *mut GraphLite) -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}
