# 教程 09 · 常见问题与排错

## 目标

- 汇总使用 NervusDB 过程中常见的问题与诊断步骤
- 提供脚本化排查工具与参考命令
- 形成运维值班时的快速手册

## 快速诊断清单

1. `nervusdb stats <db> --summary`
2. `nervusdb check <db> --summary`
3. `nervusdb txids <db> --since=60`
4. `nervusdb hot <db> --top=10`
5. `tail -n 200 logs/<service>.log`

## 常见问题列表

### 1. CLI 提示 “Database is locked”

- **现象**：写入命令报错，stats 显示 `lock: true`
- **原因**：已有进程开启 `enableLock: true`
- **处理**：确认是否双重部署；如需多写者，关闭 `enableLock` 或拆分数据库

### 2. 查询结果不稳定

- **现象**：同一查询多次执行结果不同
- **原因**：后台 compaction/GC 正在进行
- **处理**：使用 `withSnapshot` 固定 epoch；或等治理结束

### 3. WAL 文件越来越大

- **现象**：`nervusdb stats` 显示 `walBytes` > 1GB
- **原因**：长时间未 `flush` 或自动压实
- **处理**：`db.flush()`；执行 `nervusdb auto-compact --auto-gc`

### 4. manifest 缺失或损坏

- **现象**：`nervusdb stats` 报错 “manifest not found”
- **处理步骤**：
  1. 备份当前目录
  2. 执行 `nervusdb repair <db>`
  3. 如失败，`nervusdb compact <db> --rebuild`（需较长时间）

### 5. 属性索引查询无结果

- **原因**：属性未同步写入或类型不匹配
- **检查**：
  - `nervusdb dump` 查看属性 JSON
  - 确认查询条件与属性类型一致（数值/字符串）

### 6. auto-compact 跳过执行

- **日志提示**：`Skipping compaction due to X active readers`
- **解决**：
  - 查看 `docs/使用示例/07-快照一致性与并发-示例.md`
  - 让长时间快照释放；或排程在低峰期执行

### 7. GraphQL / Gremlin 查询报错

- **常见原因**：
  - 使用了未支持的语法片段
  - 传入字段与存储数据不一致
- **建议**：参考对应语法文档并先在 QueryBuilder 中验证

### 8. 批量导入性能不足

- **处理手段**：
  - 使用 `beginBatch` + `commitBatch`
  - 调整 `pageSize`、启用 LSM-Lite 暂存
  - 分批 flush，避免 WAL 过大

## 脚本工具

```bash
# scripts/diagnose.sh
DB=$1
nervusdb stats "$DB" --summary
nervusdb check "$DB" --summary || true
nervusdb txids "$DB" --since=240
nervusdb hot "$DB" --top=20
```

## 支持渠道

- GitHub Issues
- `.qoder/repowiki` 中的专题说明
- 项目讨论区（若已开放）

## 延伸阅读

- [docs/使用示例/99-常见问题与排错.md](../使用示例/99-常见问题与排错.md)
- [docs/使用示例/09-嵌入式脚本与自动化-示例.md](../使用示例/09-嵌入式脚本与自动化-示例.md)
