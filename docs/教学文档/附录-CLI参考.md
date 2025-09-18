# 附录 · CLI 参考

所有命令均可通过 `synapsedb <command>` 调用（或仓库内用 `pnpm db:*` 脚本）。

## bench

```
synapsedb bench <db> [count=10000] [mode=default|lsm]
```

## stats

```
synapsedb stats <db> [--txids[=N]] [--txids-window=MIN]
```

## auto-compact

```
synapsedb auto-compact <db> \
  [--mode=incremental|rewrite] \
  [--orders=SPO,POS,...] \
  [--min-merge=N] \
  [--tombstone-threshold=R] \
  [--hot-threshold=H] [--max-primary=K] \
  [--includeLsmSegments] [--includeLsmSegmentsAuto] \
  [--dry-run] [--auto-gc]
```

## compact（底层直驱）

```
synapsedb compact <db> [--orders=...] [--page-size=1024] [--min-merge=2] \
  [--tombstone-threshold=0.2] [--compression=brotli:4|none] [--mode=rewrite|incremental]
```

## gc（页面级）

```
synapsedb gc <db> [--no-respect-readers]
```

## check / repair

```
synapsedb check <db> [--summary|--strict]
synapsedb repair <db> [--fast]
```

## repair-page（按页）

```
synapsedb repair-page <db> <order:SPO|SOP|POS|PSO|OSP|OPS> <primary:number>
```

## dump（页导出）

```
synapsedb dump <db> <order> <primary>
```

## hot（热点）

```
synapsedb hot <db> [--order=SPO] [--top=10]
```

## txids（事务 ID 注册表）

```
synapsedb txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]
```
