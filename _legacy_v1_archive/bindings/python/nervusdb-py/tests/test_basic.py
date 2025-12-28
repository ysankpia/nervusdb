from pathlib import Path

import nervusdb


def test_basic_roundtrip(tmp_path: Path) -> None:
    db_path = tmp_path / "py_binding"
    with nervusdb.Database(str(db_path)) as db:
        db.add_fact("Alice", "knows", "Bob")
        stmt = db.prepare_v2("MATCH (a)-[r]->(b) RETURN a, r, b", None)
        try:
            assert stmt.column_count() == 3
            assert stmt.column_name(0) == "a"
            assert stmt.column_name(1) == "r"
            assert stmt.column_name(2) == "b"

            assert stmt.step() is True
            assert stmt.column_type(0) == nervusdb.ValueType.NODE
            assert stmt.column_type(1) == nervusdb.ValueType.RELATIONSHIP
            assert stmt.column_type(2) == nervusdb.ValueType.NODE

            assert stmt.column_node_id(0) is not None
            rel = stmt.column_relationship(1)
            assert rel is not None
            assert rel.predicate_id > 0

            assert stmt.step() is False
        finally:
            stmt.finalize()

        rows = list(db.prepare("MATCH (a)-[r]->(b) RETURN a, r, b"))
        assert len(rows) == 1
        assert set(rows[0].keys()) == {"a", "r", "b"}
