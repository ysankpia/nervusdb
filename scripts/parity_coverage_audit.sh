#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DATE_UTC="${BINDING_PARITY_DATE:-$(date -u +%F)}"
ART_DIR="artifacts/tck"
JSON_FILE="${ART_DIR}/parity-coverage-audit-${DATE_UTC}.json"
MD_FILE="${ART_DIR}/parity-coverage-audit-${DATE_UTC}.md"

mkdir -p "$ART_DIR"

python3 - "$ROOT_DIR" "$JSON_FILE" "$MD_FILE" <<'PY'
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

root = Path(sys.argv[1])
json_file = Path(sys.argv[2])
md_file = Path(sys.argv[3])

contract_path = root / "examples-test/capability-contract.yaml"
rust_file = root / "examples-test/nervusdb-rust-test/tests/test_capabilities.rs"
node_file = root / "examples-test/nervusdb-node-test/src/test-capabilities.ts"
py_file = root / "examples-test/nervusdb-python-test/test_capabilities.py"


def unquote(v: str) -> str:
    v = v.strip()
    if len(v) >= 2 and v[0] == "'" and v[-1] == "'":
        return v[1:-1].replace("''", "'")
    return v


def parse_contract(path: Path):
    caps = []
    cur = None
    for raw in path.read_text().splitlines():
        if raw.startswith("  - id: "):
            if cur:
                caps.append(cur)
            cur = {"id": raw.split(":", 1)[1].strip()}
            continue
        if cur is None:
            continue
        if raw.startswith("    scope: "):
            cur["scope"] = raw.split(":", 1)[1].strip()
        elif raw.startswith("    blocking: "):
            cur["blocking"] = raw.split(":", 1)[1].strip().lower() == "true"
        elif raw.startswith("    category: "):
            cur["category"] = raw.split(":", 1)[1].strip()
        elif raw.startswith("      mode: "):
            cur["mode"] = raw.split(":", 1)[1].strip()
        elif raw.startswith("      rust: "):
            cur["rust"] = unquote(raw.split(":", 1)[1])
        elif raw.startswith("      node: "):
            cur["node"] = unquote(raw.split(":", 1)[1])
        elif raw.startswith("      python: "):
            cur["python"] = unquote(raw.split(":", 1)[1])
    if cur:
        caps.append(cur)
    return caps


def parse_registry(path: Path):
    entries = {}
    duplicates = []
    line_pat = re.compile(r"(CID-SHARED-\d{3})\s*\|\s*mode=([a-z]+)\s*\|\s*case=(.+)$")
    for lineno, raw in enumerate(path.read_text().splitlines(), 1):
        m = line_pat.search(raw)
        if not m:
            continue
        cid, mode, case = m.group(1), m.group(2), m.group(3).strip()
        if cid in entries:
            duplicates.append({"id": cid, "line": lineno})
        entries[cid] = {"mode": mode, "case": case, "line": lineno}
    return entries, duplicates


def target_exists(lang: str, content: str, case_name: str) -> bool:
    if lang == "rust":
        return f"fn {case_name}(" in content
    return f'test("{case_name}"' in content


caps = parse_contract(contract_path)
shared_caps = [c for c in caps if c.get("scope") == "shared" and c.get("blocking", False)]
contract_ids = {c["id"] for c in shared_caps}
contract_by_id = {c["id"]: c for c in shared_caps}

rust_registry, rust_dups = parse_registry(rust_file)
node_registry, node_dups = parse_registry(node_file)
py_registry, py_dups = parse_registry(py_file)

rust_ids = set(rust_registry.keys())
node_ids = set(node_registry.keys())
py_ids = set(py_registry.keys())

missing_in_rust = sorted(contract_ids - rust_ids)
missing_in_node = sorted(contract_ids - node_ids)
missing_in_python = sorted(contract_ids - py_ids)

unexpected_in_rust = sorted(rust_ids - contract_ids)
unexpected_in_node = sorted(node_ids - contract_ids)
unexpected_in_python = sorted(py_ids - contract_ids)

rust_txt = rust_file.read_text()
node_txt = node_file.read_text()
py_txt = py_file.read_text()

mode_mismatch = []
target_missing = []
for cid in sorted(contract_ids):
    cap = contract_by_id[cid]
    expected_mode = cap.get("mode", "success")

    for lang, registry, text in (
        ("rust", rust_registry, rust_txt),
        ("node", node_registry, node_txt),
        ("python", py_registry, py_txt),
    ):
        entry = registry.get(cid)
        if not entry:
            continue
        if entry["mode"] != expected_mode:
            mode_mismatch.append(
                {
                    "id": cid,
                    "language": lang,
                    "expected": expected_mode,
                    "actual": entry["mode"],
                }
            )
        if not target_exists(lang, text, entry["case"]):
            target_missing.append(
                {
                    "id": cid,
                    "language": lang,
                    "case": entry["case"],
                    "line": entry["line"],
                }
            )

unexpected_cid = (
    len(unexpected_in_rust)
    + len(unexpected_in_node)
    + len(unexpected_in_python)
)

summary = {
    "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "contract_file": str(contract_path),
    "contract_shared_total": len(contract_ids),
    "missing_in_rust": len(missing_in_rust),
    "missing_in_node": len(missing_in_node),
    "missing_in_python": len(missing_in_python),
    "unexpected_cid": unexpected_cid,
    "mode_mismatch": len(mode_mismatch),
    "target_missing": len(target_missing),
    "duplicate_registry_entries": len(rust_dups) + len(node_dups) + len(py_dups),
}

details = {
    "missing": {
        "rust": missing_in_rust,
        "node": missing_in_node,
        "python": missing_in_python,
    },
    "unexpected": {
        "rust": unexpected_in_rust,
        "node": unexpected_in_node,
        "python": unexpected_in_python,
    },
    "mode_mismatch": mode_mismatch,
    "target_missing": target_missing,
    "duplicates": {
        "rust": rust_dups,
        "node": node_dups,
        "python": py_dups,
    },
}

passed = (
    summary["missing_in_rust"] == 0
    and summary["missing_in_node"] == 0
    and summary["missing_in_python"] == 0
    and summary["unexpected_cid"] == 0
    and summary["mode_mismatch"] == 0
    and summary["target_missing"] == 0
    and summary["duplicate_registry_entries"] == 0
)

payload = {"passed": passed, "summary": summary, "details": details}
json_file.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")

lines = []
lines.append(f"# Parity Coverage Audit ({json_file.stem.split('-')[-1]})")
lines.append("")
lines.append(f"- generated_at: {summary['generated_at']}")
lines.append(f"- contract_shared_total: {summary['contract_shared_total']}")
lines.append(f"- passed: {str(passed).lower()}")
lines.append("")
lines.append("| Metric | Value |")
lines.append("|---|---:|")
lines.append(f"| missing_in_rust | {summary['missing_in_rust']} |")
lines.append(f"| missing_in_node | {summary['missing_in_node']} |")
lines.append(f"| missing_in_python | {summary['missing_in_python']} |")
lines.append(f"| unexpected_cid | {summary['unexpected_cid']} |")
lines.append(f"| mode_mismatch | {summary['mode_mismatch']} |")
lines.append(f"| target_missing | {summary['target_missing']} |")
lines.append(f"| duplicate_registry_entries | {summary['duplicate_registry_entries']} |")
lines.append("")

for section, values in (
    ("Missing IDs", details["missing"]),
    ("Unexpected IDs", details["unexpected"]),
):
    lines.append(f"## {section}")
    lines.append("")
    for lang in ("rust", "node", "python"):
        vals = values[lang]
        if vals:
            sample = ", ".join(vals[:12])
            lines.append(f"- {lang}: {len(vals)} ({sample})")
        else:
            lines.append(f"- {lang}: 0")
    lines.append("")

if mode_mismatch:
    lines.append("## Mode Mismatch")
    lines.append("")
    for item in mode_mismatch[:20]:
        lines.append(
            f"- {item['id']} [{item['language']}] expected={item['expected']} actual={item['actual']}"
        )
    lines.append("")

if target_missing:
    lines.append("## Registry Target Missing")
    lines.append("")
    for item in target_missing[:20]:
        lines.append(
            f"- {item['id']} [{item['language']}] case={item['case']} (registry line {item['line']})"
        )
    lines.append("")

md_file.write_text("\n".join(lines) + "\n")

print(f"[coverage-audit] shared contract entries: {summary['contract_shared_total']}")
print(
    "[coverage-audit] "
    f"missing rust={summary['missing_in_rust']} "
    f"node={summary['missing_in_node']} "
    f"python={summary['missing_in_python']}"
)
print(
    "[coverage-audit] "
    f"unexpected={summary['unexpected_cid']} "
    f"mode_mismatch={summary['mode_mismatch']} "
    f"target_missing={summary['target_missing']} "
    f"duplicates={summary['duplicate_registry_entries']}"
)
print(f"[coverage-audit] json: {json_file}")
print(f"[coverage-audit] md: {md_file}")

sys.exit(0 if passed else 1)
PY
