#ifndef NERVUSDB_H
#define NERVUSDB_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int32_t nervusdb_status;

enum {
  NERVUSDB_OK = 0,
  NERVUSDB_ERR_INVALID_ARGUMENT = 1,
  NERVUSDB_ERR_OPEN = 2,
  NERVUSDB_ERR_INTERNAL = 3,
  NERVUSDB_ERR_CALLBACK = 4,
  // SQLite-style step() return codes (not errors).
  NERVUSDB_ROW = 100,
  NERVUSDB_DONE = 101,
};

typedef struct nervusdb_db nervusdb_db;
typedef struct nervusdb_stmt nervusdb_stmt;

typedef struct nervusdb_error {
  nervusdb_status code;
  char *message;
} nervusdb_error;

// ABI version (increment only on breaking ABI changes).
// Starting from v1.0.0, function signatures in this header are treated as a
// stable contract. Do not change without a breaking version bump and an
// explicit migration plan.
#define NERVUSDB_ABI_VERSION 1

typedef struct nervusdb_query_criteria {
  uint64_t subject_id;
  uint64_t predicate_id;
  uint64_t object_id;
  bool has_subject;
  bool has_predicate;
  bool has_object;
} nervusdb_query_criteria;

typedef bool (*nervusdb_triple_callback)(
    uint64_t subject_id,
    uint64_t predicate_id,
    uint64_t object_id,
    void *user_data);

typedef int32_t nervusdb_value_type;
enum {
  NERVUSDB_VALUE_NULL = 0,
  NERVUSDB_VALUE_TEXT = 1,
  NERVUSDB_VALUE_FLOAT = 2,
  NERVUSDB_VALUE_BOOL = 3,
  NERVUSDB_VALUE_NODE = 4,
  NERVUSDB_VALUE_RELATIONSHIP = 5,
};

typedef struct nervusdb_relationship {
  uint64_t subject_id;
  uint64_t predicate_id;
  uint64_t object_id;
} nervusdb_relationship;

// ---------------------------------------------------------------------------
// Version / memory management
// ---------------------------------------------------------------------------

// Returns the ABI version of this header/runtime (increment only on breaking changes).
uint32_t nervusdb_abi_version(void);

// Returns a NUL-terminated, static version string (do not free).
const char *nervusdb_version(void);

// Frees a string allocated by NervusDB (e.g. resolve_str / exec_cypher outputs).
void nervusdb_free_string(char *value);

nervusdb_status nervusdb_open(const char *path, nervusdb_db **out_db, nervusdb_error **out_error);
void nervusdb_close(nervusdb_db *db);

nervusdb_status nervusdb_intern(
    nervusdb_db *db,
    const char *value,
    uint64_t *out_id,
    nervusdb_error **out_error);

nervusdb_status nervusdb_resolve_id(
    nervusdb_db *db,
    const char *value,
    uint64_t *out_id,
    nervusdb_error **out_error);

nervusdb_status nervusdb_resolve_str(
    nervusdb_db *db,
    uint64_t id,
    char **out_value,
    nervusdb_error **out_error);

nervusdb_status nervusdb_add_triple(
    nervusdb_db *db,
    uint64_t subject_id,
    uint64_t predicate_id,
    uint64_t object_id,
    nervusdb_error **out_error);

nervusdb_status nervusdb_begin_transaction(nervusdb_db *db, nervusdb_error **out_error);
nervusdb_status nervusdb_commit_transaction(nervusdb_db *db, nervusdb_error **out_error);
nervusdb_status nervusdb_abort_transaction(nervusdb_db *db, nervusdb_error **out_error);

nervusdb_status nervusdb_query_triples(
    nervusdb_db *db,
    const nervusdb_query_criteria *criteria,
    nervusdb_triple_callback callback,
    void *user_data,
    nervusdb_error **out_error);

nervusdb_status nervusdb_exec_cypher(
    nervusdb_db *db,
    const char *query,
    const char *params_json,
    char **out_json,
    nervusdb_error **out_error);

// ---------------------------------------------------------------------------
// Cypher statement API (SQLite-style, row iterator)
//
// Ownership / lifetime:
// - `nervusdb_column_name()` pointer is valid until `nervusdb_finalize()`.
// - `nervusdb_column_text()` pointer is valid until the next `nervusdb_step()`
//   call on the same statement, or `nervusdb_finalize()`.
// - Callers MUST NOT free column pointers (do not use `nervusdb_free_string`).
// ---------------------------------------------------------------------------

nervusdb_status nervusdb_prepare_v2(
    nervusdb_db *db,
    const char *query,
    const char *params_json,
    nervusdb_stmt **out_stmt,
    nervusdb_error **out_error);

// Returns:
// - NERVUSDB_ROW: a row is available (use `nervusdb_column_*`)
// - NERVUSDB_DONE: no more rows
// - otherwise: error (see out_error)
nervusdb_status nervusdb_step(nervusdb_stmt *stmt, nervusdb_error **out_error);

int32_t nervusdb_column_count(nervusdb_stmt *stmt);
const char *nervusdb_column_name(nervusdb_stmt *stmt, int32_t column);
nervusdb_value_type nervusdb_column_type(nervusdb_stmt *stmt, int32_t column);

const char *nervusdb_column_text(nervusdb_stmt *stmt, int32_t column);
int32_t nervusdb_column_bytes(nervusdb_stmt *stmt, int32_t column);
double nervusdb_column_double(nervusdb_stmt *stmt, int32_t column);
int32_t nervusdb_column_bool(nervusdb_stmt *stmt, int32_t column);
uint64_t nervusdb_column_node_id(nervusdb_stmt *stmt, int32_t column);
nervusdb_relationship nervusdb_column_relationship(nervusdb_stmt *stmt, int32_t column);

void nervusdb_finalize(nervusdb_stmt *stmt);

void nervusdb_free_error(nervusdb_error *error);

#ifdef __cplusplus
}
#endif

#endif /* NERVUSDB_H */
