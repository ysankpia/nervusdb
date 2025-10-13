# 示例 00 · CLI 全局快速开始

## 目标

- 通过命令行完成数据库创建、导入、统计、压实与导出
- 熟悉 `nervusdb` 命令的常用参数与输出格式

## 前置准备

```bash
pnpm build
npm i -g .    # 或使用 npx nervusdb
mkdir -p ~/data/sdb
cd ~/data/sdb
```

## 1. 生成示例数据库

```bash
nervusdb bench demo.nervusdb 200 lsm
```

输出示例：

```
🚀 生成示例数据... triples=820, properties=400
✅ 完成：demo.nervusdb + demo.nervusdb.pages/
```

## 2. 查看统计

```bash
nervusdb stats demo.nervusdb --summary
nervusdb stats demo.nervusdb --txids=10
```

重点字段：`triples`、`tombstones`、`walBytes`、`orders.*.multiPagePrimaries`

## 3. 自动压实

```bash
nervusdb auto-compact demo.nervusdb \
  --mode=incremental \
  --hot-threshold=1.1 \
  --max-primary=5 \
  --auto-gc
```

日志示例：

```
📊 Manifest summary: Total lookups: 6, Page size: 1024
🔥 Hotness primary 42 score 0.87 -> selected
✅ Compaction completed: Pages before 12 → after 8
```

## 4. 查看热点

```bash
nervusdb hot demo.nervusdb --top=10
```

输出示例：`primary=42 pages=3 score=0.82`

## 5. 导出页内容

```bash
nervusdb dump demo.nervusdb SPO 42 --output spo-42.ndjson
head spo-42.ndjson
```

## 6. 事务 ID 管理

```bash
nervusdb txids demo.nervusdb --list=10
nervusdb txids demo.nervusdb --since=240
```

## 7. 快速检查

```bash
nervusdb check demo.nervusdb --summary
```

若需深度校验：`nervusdb check demo.nervusdb --strict`

## 8. 清理示例

```bash
rm -rf demo.nervusdb demo.nervusdb.pages demo.nervusdb.wal spo-42.ndjson
```

## 小贴士

- `--json` 可输出 JSON，便于 `jq` 处理
- `--dry-run` 预览 auto-compact 操作
- 在 CI 中使用时，可将命令写入脚本并捕获退出码

## 延伸阅读

- [docs/教学文档/教程-06-维护与治理.md](../教学文档/教程-06-维护与治理.md)
- [docs/教学文档/附录-CLI参考.md](../教学文档/附录-CLI参考.md)
