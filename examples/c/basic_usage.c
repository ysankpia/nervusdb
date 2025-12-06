#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "../../nervusdb-core/include/nervusdb.h"

static bool print_triple(uint64_t subject, uint64_t predicate, uint64_t object, void *user_data) {
  (void)user_data;
  printf("triple => (%llu, %llu, %llu)\n",
         (unsigned long long)subject,
         (unsigned long long)predicate,
         (unsigned long long)object);
  return true;
}

int main(void) {
  nervusdb_db *db = NULL;
  nervusdb_error *err = NULL;

  if (nervusdb_open("./example_db", &db, &err) != NERVUSDB_OK) {
    fprintf(stderr, "Failed to open database: %s\n", err && err->message ? err->message : "unknown");
    nervusdb_free_error(err);
    return 1;
  }

  uint64_t alice_id = 0;
  if (nervusdb_intern(db, "Alice", &alice_id, &err) != NERVUSDB_OK) {
    fprintf(stderr, "intern failed: %s\n", err && err->message ? err->message : "unknown");
    nervusdb_free_error(err);
    nervusdb_close(db);
    return 1;
  }

  if (nervusdb_add_triple(db, alice_id, alice_id, alice_id, &err) != NERVUSDB_OK) {
    fprintf(stderr, "add_triple failed: %s\n", err && err->message ? err->message : "unknown");
    nervusdb_free_error(err);
    nervusdb_close(db);
    return 1;
  }

  nervusdb_query_criteria criteria = {
    .subject_id = alice_id,
    .predicate_id = 0,
    .object_id = 0,
    .has_subject = true,
    .has_predicate = false,
    .has_object = false,
  };

  if (nervusdb_query_triples(db, &criteria, print_triple, NULL, &err) != NERVUSDB_OK) {
    fprintf(stderr, "query failed: %s\n", err && err->message ? err->message : "unknown");
    nervusdb_free_error(err);
    nervusdb_close(db);
    return 1;
  }

  nervusdb_close(db);
  return 0;
}
