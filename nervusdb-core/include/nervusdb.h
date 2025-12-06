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
};

typedef struct nervusdb_db nervusdb_db;

typedef struct nervusdb_error {
  nervusdb_status code;
  char *message;
} nervusdb_error;

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

nervusdb_status nervusdb_open(const char *path, nervusdb_db **out_db, nervusdb_error **out_error);
void nervusdb_close(nervusdb_db *db);

nervusdb_status nervusdb_intern(
    nervusdb_db *db,
    const char *value,
    uint64_t *out_id,
    nervusdb_error **out_error);

nervusdb_status nervusdb_add_triple(
    nervusdb_db *db,
    uint64_t subject_id,
    uint64_t predicate_id,
    uint64_t object_id,
    nervusdb_error **out_error);

nervusdb_status nervusdb_query_triples(
    nervusdb_db *db,
    const nervusdb_query_criteria *criteria,
    nervusdb_triple_callback callback,
    void *user_data,
    nervusdb_error **out_error);

void nervusdb_free_error(nervusdb_error *error);

#ifdef __cplusplus
}
#endif

#endif /* NERVUSDB_H */
