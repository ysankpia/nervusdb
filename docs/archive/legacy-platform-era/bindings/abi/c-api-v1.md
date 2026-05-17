# NervusDB C ABI v1（稳定公开契约）

本文档定义 NervusDB 对外稳定 C ABI（v1）以及兼容规则。  
本版本以 `nervusdb-capi` crate 导出的 `libnervusdb` + `nervusdb.h` 为唯一跨语言契约来源。

## 1. 稳定性边界

- 目标版本：`v1.x`
- 稳定承诺：
  - 已发布符号在同一 Major 版本内只增不改不删
  - 错误码和错误分类保持稳定
  - 句柄类型保持 opaque（不公开内存布局）
- Header 来源：`cbindgen` 自动生成 `nervusdb-c-sdk/include/nervusdb.h`

## 2. 句柄与生命周期

- 句柄：
  - `ndb_db_t`
  - `ndb_stmt_t`
  - `ndb_txn_t`
  - `ndb_result_t`
- 生命周期 API：
  - `ndb_open(path, out_db)`
  - `ndb_open_paths(ndb_path, wal_path, out_db)`
  - `ndb_close(db)`
- 错误读取 API（线程局部）：
  - `ndb_last_error_code()`
  - `ndb_last_error_category()`
  - `ndb_last_error_message(buf, len)`

## 3. 查询/写入接口

- 便捷 API：
  - `ndb_query(db, cypher, params_json, out_result)`（仅允许读语句）
  - `ndb_execute_write(db, cypher, params_json, out_summary)`（仅允许写语句）
- 预处理 API：
  - `ndb_prepare_read(...)`
  - `ndb_prepare_write(...)`
  - `ndb_stmt_bind_*`
  - `ndb_stmt_step(...)`
  - `ndb_stmt_column_*`
  - `ndb_stmt_reset(...)`
  - `ndb_stmt_finalize(...)`

## 4. 事务与低层接口（v1）

- 事务：
  - `ndb_begin_write`
  - `ndb_txn_commit`
  - `ndb_txn_rollback`
  - `ndb_txn_query`
- 低层写接口：
  - `ndb_txn_create_node`
  - `ndb_txn_get_or_create_label`
  - `ndb_txn_get_or_create_rel_type`
  - `ndb_txn_create_edge`
  - `ndb_txn_tombstone_node`
  - `ndb_txn_tombstone_edge`
  - `ndb_txn_set_node_property`
  - `ndb_txn_set_edge_property`
  - `ndb_txn_remove_node_property`
  - `ndb_txn_remove_edge_property`
  - `ndb_txn_set_vector`

## 5. 维护与高级接口（v1）

- DB 级接口：
  - `ndb_compact`
  - `ndb_checkpoint`
  - `ndb_create_index`
  - `ndb_search_vector`
- 顶层接口：
  - `ndb_vacuum`
  - `ndb_backup`
  - `ndb_bulkload`

## 6. 错误契约

- 分类（稳定整数）：
  - `NDB_ERRCAT_SYNTAX`
  - `NDB_ERRCAT_EXECUTION`
  - `NDB_ERRCAT_STORAGE`
  - `NDB_ERRCAT_COMPATIBILITY`
- 错误码（稳定整数）：
  - `NDB_OK`
  - `NDB_ERR_INVALID_ARGUMENT`
  - `NDB_ERR_NULL_POINTER`
  - `NDB_ERR_SYNTAX`
  - `NDB_ERR_EXECUTION`
  - `NDB_ERR_STORAGE`
  - `NDB_ERR_COMPATIBILITY`
  - `NDB_ERR_BUSY`
  - `NDB_ERR_UNSUPPORTED`
  - `NDB_ERR_INTERNAL`

说明：`message` 仅用于诊断，不作为兼容基线。

## 7. 内存与线程语义

- 内存所有权：
  - 由 ABI 分配并返回的 `char*` 必须调用 `ndb_string_free`
  - 结果句柄必须调用 `ndb_result_free`
- 线程语义：
  - `db` 允许并发读
  - `txn` 句柄不得跨线程共享
  - 活跃事务存在时 `ndb_close` 返回 `NDB_ERR_BUSY`
