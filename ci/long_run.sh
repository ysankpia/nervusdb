#!/usr/bin/env bash
#
# 长时测试战役：把所有测试工具按阶段跑一遍，**失败不中断、结果可追溯**。
#
# ## 为什么要一个脚本
#
# 单个 `cargo test` 覆盖的是「已知要测的东西」。这一轮发现的九个静默缺陷里，没有
# 一个是它抓到的——它们都在「没人跑过的组合」里。这个脚本的作用是把那些组合**编排
# 成一次可长跑的任务**，并且每一阶段都留下日志，失败时能直接看到是哪一类。
#
# 参照的是 SQLite 的测试哲学（差集测试、模糊测试、故障注入、崩溃注入、边界穷举），
# 而不是它的体量——SQLite 的测试代码是源码的数百倍，那是二十多年的积累。
#
# ## 阶段
#
#   1. full       —— Rust 全量套件（含 5 项门禁）
#   2. fuzz       —— Cypher 模糊测试，多个种子区间
#   3. fault      —— 外部故障注入
#   4. boundaries —— 页/记录密度等结构性边界（两侧各测一次）
#   5. differential —— 写操作对拍内存模型（长时间）
#   6. crash      —— SIGKILL 崩溃恢复多轮
#   7. concurrency—— 并发扩展与压力
#   8. sdk        —— 两个 SDK 端到端 + 跨 SDK 对照
#   9. realdata   —— com-DBLP 红线（需要数据集，缺失则跳过并说明）
#
# ## 用法
#
#   ci/long_run.sh                  # 全部阶段，默认时长
#   ci/long_run.sh fuzz fault       # 只跑指定阶段
#   LONG=1 ci/long_run.sh           # 加长版（更多种子、更多轮次）
#
# 日志写到 `target/long-run/<时间戳>/`，`SUMMARY.txt` 汇总每个阶段的结果。

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="$REPO/target/long-run/$STAMP"
mkdir -p "$OUT"

LONG="${LONG:-0}"
if [ "$LONG" = "1" ]; then
	FUZZ_SEEDS="${FUZZ_SEEDS:-400}"
	DIFF_STEPS="${DIFF_STEPS:-200000}"
	CRASH_ROUNDS="${CRASH_ROUNDS:-20}"
else
	FUZZ_SEEDS="${FUZZ_SEEDS:-120}"
	DIFF_STEPS="${DIFF_STEPS:-40000}"
	CRASH_ROUNDS="${CRASH_ROUNDS:-8}"
fi

ALL_STAGES=(full fuzz fault boundaries differential crash concurrency sdk realdata)
STAGES=("$@")
if [ ${#STAGES[@]} -eq 0 ]; then
	STAGES=("${ALL_STAGES[@]}")
fi

SUMMARY="$OUT/SUMMARY.txt"
: >"$SUMMARY"

# 记录每个阶段的结果：name|status|seconds|log
PASS=0
FAIL=0
SKIP=0

log() { printf '%s\n' "$*" | tee -a "$SUMMARY"; }

banner() {
	echo
	echo "================================================================"
	echo "  $*"
	echo "================================================================"
}

# 跑一个阶段：命令成功记 PASS，失败记 FAIL 并**继续**（不中断整轮）。
run_stage() {
	local name="$1"
	shift
	local logfile="$OUT/$name.log"
	banner "STAGE: $name"
	echo "  log: $logfile"
	local start end
	start=$(date +%s)
	if "$@" >"$logfile" 2>&1; then
		end=$(date +%s)
		PASS=$((PASS + 1))
		printf 'PASS  %-14s %4ds  %s\n' "$name" "$((end - start))" "$logfile" | tee -a "$SUMMARY"
		# 回显关键行，便于直接在终端看到结论
		grep -E "PASSED|FAILED|ok |test result|Failures|Checked|total|红线|red line" "$logfile" | tail -8 || true
	else
		end=$(date +%s)
		FAIL=$((FAIL + 1))
		printf 'FAIL  %-14s %4ds  %s\n' "$name" "$((end - start))" "$logfile" | tee -a "$SUMMARY"
		echo "  --- last 30 lines of the failure ---"
		tail -30 "$logfile" | sed 's/^/    /' || true
	fi
}

# 跳过（附原因），不计入失败。
skip_stage() {
	local name="$1" why="$2"
	SKIP=$((SKIP + 1))
	printf 'SKIP  %-14s %s\n' "$name" "$why" | tee -a "$SUMMARY"
}

# ---------------------------------------------------------------------------
# 阶段实现
# ---------------------------------------------------------------------------

stage_full() {
	set -e
	echo "--- fmt ---"
	cargo fmt --all -- --check
	echo "--- check ---"
	RUSTFLAGS="-D warnings" cargo check --workspace --all-targets
	echo "--- clippy ---"
	cargo clippy --workspace --all-targets -- -D warnings
	echo "--- rustdoc ---"
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
	echo "--- tests ---"
	cargo test --workspace
	echo "--- throughput suites (need --release to mean anything) ---"
	cargo test --release --test batch_tx_tests
	cargo test --release --test edge_locality_tests
}

stage_fuzz() {
	set -e
	# 分多段跑，逐步扩大规模：小图先把形状扫全，大图再压分页路径。
	echo "--- fuzz: seeds 1..$FUZZ_SEEDS ---"
	SEED0=1 SEEDS="$FUZZ_SEEDS" cargo bench --bench cypher_fuzz_bench
	echo "--- fuzz: a second, disjoint seed range (different graphs) ---"
	SEED0=$((FUZZ_SEEDS + 1)) SEEDS="$FUZZ_SEEDS" cargo bench --bench cypher_fuzz_bench
}

stage_fault() {
	set -e
	cargo bench --bench fault_injection_bench
}

# 结构性魔数边界：记录密度（128/64 每页）、直接目录页覆盖（4096 节点 / 2048 边）、
# 属性页槽位、1KB 内联/溢出分界。越界出错时症状是「读到别的实体的数据」，因此每个
# 边界**两侧各测一次**。注意：这个阶段与寻址常量守卫配套——那条守卫在 `full` 里，
# 因为「改常量」这种改动行为测试根本看不到（见该测试的注释）。
stage_boundaries() {
	set -e
	cargo bench --bench page_boundary_bench
}

stage_differential() {
	set -e
	DB_DIR="$OUT/diff" STEPS="$DIFF_STEPS" cargo bench --bench differential_test
}

stage_crash() {
	set -e
	set +e
	# 现有的崩溃工具自带轮数与校验；多跑几轮以覆盖更多中断点。
	for i in $(seq 1 "$CRASH_ROUNDS"); do
		echo "--- crash round $i/$CRASH_ROUNDS ---"
		DB_DIR="$OUT/crash$i" cargo bench --bench crash_recovery_test || {
			echo "crash round $i FAILED"
			exit 1
		}
	done
}

stage_concurrency() {
	set -e
	echo "--- synthetic read scaling (no dataset needed) ---"
	cargo bench --bench concurrency_scaling_bench
	echo "--- correctness under concurrent stress ---"
	cargo test --release --test concurrency_stress_tests
	cargo test --release --test concurrency_isolation_tests
	cargo test --release --test multi_process_write_tests
}

stage_sdk() {
	set -e
	cargo build -p nervusdb-node -p nervusdb-python
	# 运行时只看文件扩展名，按平台取产物。
	local node_art python_art
	if [ -f target/debug/libnervusdb_node.dylib ]; then
		node_art=target/debug/libnervusdb_node.dylib
		python_art=target/debug/libnervusdb_python.dylib
	else
		node_art=target/debug/libnervusdb_node.so
		python_art=target/debug/libnervusdb_python.so
	fi
	cp "$node_art" bindings/nodejs/nervusdb.node
	cp "$python_art" bindings/python/nervusdb.so

	echo "--- Node end-to-end ---"
	node bindings/nodejs/test.mjs
	echo "--- Python end-to-end ---"
	(cd bindings/python && PYTHONPATH=. python3 tests/test_nervusdb.py)
	echo "--- cross-SDK agreement ---"
	python3 bindings/cross_sdk_check.py
}

stage_realdata() {
	local ds="${DATASET_PATH:-/Volumes/WorkDrive/Datasets/com-dblp.ungraph.txt}"
	if [ ! -f "$ds" ]; then
		echo "dataset not found at $ds — nothing to run"
		return 42 # 由调用方转成 SKIP
	fi
	set -e
	DATASET_PATH="$ds" DB_DIR="$OUT/dblp" POOL_MB=256 AUTO_CHECKPOINT_MB=0 \
		cargo bench --bench snap_dblp_bench
}

# ---------------------------------------------------------------------------
# 主循环
# ---------------------------------------------------------------------------

banner "LONG RUN $STAMP"
log "repo        : $REPO"
log "output      : $OUT"
log "mode        : $([ "$LONG" = 1 ] && echo LONG || echo default)"
log "fuzz seeds  : $FUZZ_SEEDS (x2 ranges)"
log "diff steps  : $DIFF_STEPS"
log "crash rounds: $CRASH_ROUNDS"
log "stages      : ${STAGES[*]}"
log ""

for stage in "${STAGES[@]}"; do
	case "$stage" in
	full) run_stage full stage_full ;;
	fuzz) run_stage fuzz stage_fuzz ;;
	fault) run_stage fault stage_fault ;;
	boundaries) run_stage boundaries stage_boundaries ;;
	differential) run_stage differential stage_differential ;;
	crash) run_stage crash stage_crash ;;
	concurrency) run_stage concurrency stage_concurrency ;;
	sdk) run_stage sdk stage_sdk ;;
	realdata)
		# 数据集缺失是环境问题，不是产品缺陷，因此记为 SKIP。
		run_stage realdata stage_realdata
		if [ $? -eq 42 ]; then
			FAIL=$((FAIL - 1))
			PASS=$((PASS - 1))
			SKIP=$((SKIP + 1))
			log "SKIP  realdata       dataset missing"
		fi
		;;
	*)
		echo "unknown stage: $stage" >&2
		exit 2
		;;
	esac
done

banner "SUMMARY"
cat "$SUMMARY"
echo
echo "passed=$PASS  failed=$FAIL  skipped=$SKIP"
echo "logs: $OUT"

if [ "$FAIL" -gt 0 ]; then
	echo "LONG RUN FAILED"
	exit 1
fi
echo "LONG RUN PASSED"
