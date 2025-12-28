from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Iterator, Optional

from nervusdb_uniffi.nervusdb import Database as _Database
from nervusdb_uniffi.nervusdb import NervusError, Relationship, Statement, ValueType


@dataclass(frozen=True)
class CypherSummary:
    statement: str
    parameters_json: Optional[str]
    native: bool = True


@dataclass(frozen=True)
class CypherResult:
    records: list[dict[str, Any]]
    summary: CypherSummary


class Database:
    def __init__(self, path: str):
        self._inner = _Database(path)

    def close(self) -> None:
        self._inner.close()

    def __enter__(self) -> "Database":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        self.close()
        return False

    def add_fact(self, subject: str, predicate: str, object: str) -> Relationship:
        return self._inner.add_fact(subject, predicate, object)

    def prepare_v2(self, cypher: str, params_json: Optional[str] = None) -> Statement:
        return self._inner.prepare_v2(cypher, params_json)

    def prepare(self, cypher: str, params_json: Optional[str] = None) -> "StatementRows":
        return StatementRows(self.prepare_v2(cypher, params_json), cypher, params_json)


def _row_from_statement(stmt: Statement) -> Dict[str, Any]:
    count = stmt.column_count()
    out: Dict[str, Any] = {}
    for i in range(count):
        name = stmt.column_name(i) or f"col{i}"
        t = stmt.column_type(i)
        if t == ValueType.NULL:
            out[name] = None
        elif t == ValueType.TEXT:
            out[name] = stmt.column_text(i)
        elif t == ValueType.FLOAT:
            out[name] = stmt.column_double(i)
        elif t == ValueType.BOOL:
            out[name] = stmt.column_bool(i)
        elif t == ValueType.NODE:
            out[name] = stmt.column_node_id(i)
        elif t == ValueType.RELATIONSHIP:
            out[name] = stmt.column_relationship(i)
        else:
            out[name] = None
    return out


class StatementRows(Iterator[Dict[str, Any]]):
    def __init__(self, stmt: Statement, cypher: str, params_json: Optional[str]):
        self._stmt = stmt
        self._done = False
        self._summary = CypherSummary(statement=cypher, parameters_json=params_json)

    @property
    def statement(self) -> Statement:
        return self._stmt

    @property
    def summary(self) -> CypherSummary:
        return self._summary

    def __iter__(self) -> "StatementRows":
        return self

    def __next__(self) -> Dict[str, Any]:
        if self._done:
            raise StopIteration
        has_row = self._stmt.step()
        if not has_row:
            self._done = True
            self._stmt.finalize()
            raise StopIteration
        return _row_from_statement(self._stmt)

