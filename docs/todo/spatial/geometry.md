## TODO: 严格几何计算与拓扑改进

范围：`src/spatial/geometry.ts`

待办项：

1. 严格 `intersects/contains/within` 实现
   - 现在多处基于 bbox 或近似；引入稳健谓词（robust predicates），支持边界/自交/洞等复杂情况。
2. `buffer()` 严谨实现
   - 由 bbox 扩展替换为按圆弧/线段偏移的真实缓冲；支持联合/差集与洞。
3. `simplify()` 保拓扑版本
   - Douglas–Peucker 增强为拓扑保留（防止自交/反向等）。
4. 椭球模型面积/长度
   - 现多处使用平均半径近似；增加可选的椭球算法与单位换算精度测试。
5. `isValid/makeValid`
   - 增加自交/重复点/方向性等检查与修正策略。

验收标准：

- 提供 `strictGeometry` 选项开启严格计算；
- 典型边界用例（含洞、多环、多部件）有单测；
- 默认性能不退化（严格模式默认关闭）。
