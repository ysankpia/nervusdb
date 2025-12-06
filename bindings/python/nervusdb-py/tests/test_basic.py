from pathlib import Path

from nervusdb import DatabaseHandle, open as open_db


def test_basic_roundtrip(tmp_path: Path) -> None:
    db_path = tmp_path / "py_binding"

    with DatabaseHandle(str(db_path)) as db:
        alice = db.intern("Alice")
        bob = db.intern("Bob")
        knows = db.intern("knows")

        triple = db.add_fact("Alice", "knows", "Bob")
        assert triple == (alice, knows, bob)

        inserted = db.batch_add_triples([(alice, knows, bob), (bob, knows, alice)])
        assert inserted >= 1

        results = db.query(subject=alice, predicate=None, object=None)
        assert any(item[2] == bob for item in results)

        deleted = db.batch_delete_triples([(bob, knows, alice)])
        assert deleted == 1


def test_open_function(tmp_path: Path) -> None:
    db = open_db(str(tmp_path / "open_fn"))
    try:
        assert db.intern("Carol") > 0
    finally:
        db.close()
