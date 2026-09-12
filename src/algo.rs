use crate::disk_graph::DiskGraph;
use crate::graph::{Direction, Edge, GraphError};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

/// 用于 Dijkstra 优先队列的小顶堆状态结构体
#[derive(Copy, Clone, PartialEq)]
struct State {
    cost: f64,
    node: u64,
}

impl Eq for State {}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // 反向排序实现最小堆
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 无权 BFS 最短路径算法（寻找跳数最少的路径）
/// 返回节点 ID 序列 [start_id, ..., end_id]
pub fn bfs_shortest_path(
    graph: &DiskGraph,
    start_id: u64,
    end_id: u64,
    edge_type_filter: Option<&str>,
) -> Option<Vec<u64>> {
    // 校验起点和终点是否存在
    if graph.read_node_record(start_id).ok().flatten().is_none()
        || graph.read_node_record(end_id).ok().flatten().is_none()
    {
        return None;
    }

    if start_id == end_id {
        return Some(vec![start_id]);
    }

    let mut visited: HashSet<u64> = HashSet::new();
    let mut parent_map: HashMap<u64, u64> = HashMap::new();
    let mut queue: VecDeque<u64> = VecDeque::new();

    visited.insert(start_id);
    queue.push_back(start_id);

    let mut found = false;

    while let Some(current_id) = queue.pop_front() {
        if current_id == end_id {
            found = true;
            break;
        }

        if let Ok(edges) = graph.outgoing_edges(current_id) {
            for edge in edges {
                if let Some(filter) = edge_type_filter {
                    if edge.edge_type != filter {
                        continue;
                    }
                }

                let neighbor = edge.dst_id;
                if visited.insert(neighbor) {
                    parent_map.insert(neighbor, current_id);
                    queue.push_back(neighbor);

                    if neighbor == end_id {
                        found = true;
                        break;
                    }
                }
            }
        }

        if found {
            break;
        }
    }

    if !found {
        return None;
    }

    let mut path = Vec::new();
    let mut curr = end_id;
    path.push(curr);

    while curr != start_id {
        if let Some(&p) = parent_map.get(&curr) {
            path.push(p);
            curr = p;
        } else {
            return None;
        }
    }

    path.reverse();
    Some(path)
}

/// 带权 Dijkstra 最短路径算法
/// 返回 (总权重, 节点 ID 路径序列)
pub fn dijkstra_shortest_path(
    graph: &DiskGraph,
    start_id: u64,
    end_id: u64,
    edge_type_filter: Option<&str>,
) -> Option<(f64, Vec<u64>)> {
    if graph.read_node_record(start_id).ok().flatten().is_none()
        || graph.read_node_record(end_id).ok().flatten().is_none()
    {
        return None;
    }

    if start_id == end_id {
        return Some((0.0, vec![start_id]));
    }

    let mut dist: HashMap<u64, f64> = HashMap::new();
    let mut prev: HashMap<u64, u64> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start_id, 0.0);
    heap.push(State {
        cost: 0.0,
        node: start_id,
    });

    while let Some(State {
        cost,
        node: current,
    }) = heap.pop()
    {
        if current == end_id {
            let mut path = Vec::new();
            let mut curr = end_id;
            path.push(curr);

            while curr != start_id {
                if let Some(&p) = prev.get(&curr) {
                    path.push(p);
                    curr = p;
                } else {
                    return None;
                }
            }

            path.reverse();
            return Some((cost, path));
        }

        let current_best_dist = *dist.get(&current).unwrap_or(&f64::INFINITY);
        if cost > current_best_dist {
            continue;
        }

        if let Ok(edges) = graph.outgoing_edges(current) {
            for edge in edges {
                if let Some(filter) = edge_type_filter {
                    if edge.edge_type != filter {
                        continue;
                    }
                }

                let next = edge.dst_id;
                let next_cost = cost + edge.weight;

                if next_cost < *dist.get(&next).unwrap_or(&f64::INFINITY) {
                    dist.insert(next, next_cost);
                    prev.insert(next, current);
                    heap.push(State {
                        cost: next_cost,
                        node: next,
                    });
                }
            }
        }
    }

    None
}

/// 有向图环路检测算法：三色标记法
#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    White, // 未访问
    Gray,  // 正在递归栈中
    Black, // 已完全遍历完其分支
}

/// 检测全图是否存在有向环路
pub fn has_cycle(graph: &DiskGraph) -> bool {
    let node_ids = match graph.all_node_ids() {
        Ok(ids) => ids,
        Err(_) => return false,
    };

    let mut color_map: HashMap<u64, Color> = HashMap::new();
    for &node_id in &node_ids {
        color_map.insert(node_id, Color::White);
    }

    for &node_id in &node_ids {
        if color_map.get(&node_id) == Some(&Color::White)
            && dfs_has_cycle_iterative(graph, node_id, &mut color_map)
        {
            return true;
        }
    }

    false
}

/// 迭代式深度优先环检测（显式栈）。
///
/// ## 为什么不能用递归
///
/// 递归版本的栈深等于**路径长度**，而这是用户数据决定的：一条 6 万节点的链
/// （完全合法，且是「作者-作品-引用」这类数据的自然形态）会直接耗尽线程栈。
///
/// 实测：`has_cycle()` 在 6 万节点长链上触发
/// `thread 'main' has overflowed its stack / fatal runtime error: stack overflow`，
/// 也就是**进程级 abort**——不可捕获、不可恢复。对一个嵌入式库来说，让合法数据
/// 触发进程崩溃是不可接受的。
///
/// 改为显式栈后，内存用量在堆上按需增长，与数据规模成正比而非与栈上限相关。
/// 着色语义与递归版完全一致（White→Gray 入栈、Gray→Black 出栈，遇到 Gray 即成环）。
fn dfs_has_cycle_iterative(
    graph: &DiskGraph,
    start: u64,
    color_map: &mut HashMap<u64, Color>,
) -> bool {
    // (节点, 是否已完成其全部邻居)
    let mut stack: Vec<(u64, bool)> = vec![(start, false)];
    color_map.insert(start, Color::Gray);

    while let Some((node_id, expanded)) = stack.pop() {
        if expanded {
            // 邻居已全部处理完，标记为 Black 并出栈
            color_map.insert(node_id, Color::Black);
            continue;
        }

        // 重新压入自己并标记为「待完成」，这样它会在所有子节点之后被处理。
        // 必须重新压入而不是修改栈顶：邻居也要压栈，必须夹在中间。
        stack.push((node_id, true));

        if let Ok(neighbors) = graph.neighbors(node_id, crate::graph::Direction::Outgoing) {
            for neighbor in neighbors {
                match color_map.get(&neighbor).copied() {
                    Some(Color::Gray) => return true,
                    Some(Color::White) => {
                        color_map.insert(neighbor, Color::Gray);
                        stack.push((neighbor, false));
                    }
                    _ => {}
                }
            }
        }
    }

    false
}

/// 查找全图所有有向环路
pub fn find_cycles(graph: &DiskGraph) -> Vec<Vec<u64>> {
    let node_ids = match graph.all_node_ids() {
        Ok(ids) => ids,
        Err(_) => return Vec::new(),
    };

    let mut color_map: HashMap<u64, Color> = HashMap::new();
    for &node_id in &node_ids {
        color_map.insert(node_id, Color::White);
    }

    let mut cycles = Vec::new();

    for &node_id in &node_ids {
        if color_map.get(&node_id) == Some(&Color::White) {
            dfs_find_cycles_iterative(graph, node_id, &mut color_map, &mut cycles);
        }
    }

    cycles
}

/// 迭代式环查找（显式栈），与 `has_cycle` 同理：递归深度等于路径长度，
/// 长链会让进程栈溢出。这里改为在堆上维护显式栈与当前路径。
///
/// 着色语义与递归版一致：White→Gray 入路径，Gray 命中即记录环，出栈时置 Black。
fn dfs_find_cycles_iterative(
    graph: &DiskGraph,
    start: u64,
    color_map: &mut HashMap<u64, Color>,
    cycles: &mut Vec<Vec<u64>>,
) {
    // (节点, 邻居列表, 下一个待处理邻居的下标)
    let mut stack: Vec<(u64, Vec<u64>, usize)> = Vec::new();
    let mut path_stack: Vec<u64> = Vec::new();

    color_map.insert(start, Color::Gray);
    path_stack.push(start);
    let first_neighbors = graph
        .neighbors(start, crate::graph::Direction::Outgoing)
        .unwrap_or_default();
    stack.push((start, first_neighbors, 0));

    while let Some((node_id, neighbors, idx)) = stack.last_mut() {
        let node_id = *node_id;
        if *idx >= neighbors.len() {
            // 该节点的邻居已处理完：出路径、置 Black
            path_stack.pop();
            color_map.insert(node_id, Color::Black);
            stack.pop();
            continue;
        }

        let neighbor = neighbors[*idx];
        *idx += 1;

        match color_map.get(&neighbor).copied() {
            Some(Color::Gray) => {
                // 命中当前路径上的节点 → 记录这个环
                if let Some(pos) = path_stack.iter().position(|&x| x == neighbor) {
                    let mut cycle = path_stack[pos..].to_vec();
                    cycle.push(neighbor);
                    cycles.push(cycle);
                }
            }
            Some(Color::White) => {
                color_map.insert(neighbor, Color::Gray);
                path_stack.push(neighbor);
                let nbrs = graph
                    .neighbors(neighbor, crate::graph::Direction::Outgoing)
                    .unwrap_or_default();
                stack.push((neighbor, nbrs, 0));
            }
            _ => {}
        }
    }
}

/// PageRank 打分结果（节点 ID 与归一化影响力分数）
#[derive(Debug, Clone, PartialEq)]
pub struct PageRankScore {
    pub node_id: u64,
    pub score: f64,
}

/// PageRank 阻尼迭代算法（纯磁盘邻接表 + 缓冲池按需换页，零全图驻留）。
///
/// - `damping_factor`：阻尼系数 d，通常取 0.85；
/// - `max_iterations`：最大迭代轮数上限（同时作为收敛硬止损）；
/// - `tolerance`：L1 收敛容差，相邻两轮总分变化小于该阈值即提前收敛。
///
/// 返回按分数降序排列的结果；分数总和恒为 1.0（含悬挂节点质量再分配）。
pub fn pagerank(
    graph: &DiskGraph,
    damping_factor: f64,
    max_iterations: usize,
    tolerance: f64,
) -> Vec<PageRankScore> {
    let node_ids = match graph.all_node_ids() {
        Ok(ids) => ids,
        Err(_) => return Vec::new(),
    };
    if node_ids.is_empty() {
        return Vec::new();
    }

    let n = node_ids.len();
    let d = damping_factor.clamp(0.0, 1.0);
    let base = (1.0 - d) / n as f64;

    // 预收集出边目标，避免每轮重复解析边属性溢出页（仍是磁盘流式游标驱动）
    let mut out_targets: HashMap<u64, Vec<u64>> = HashMap::with_capacity(n);
    for &nid in &node_ids {
        let mut targets = Vec::new();
        if let Ok(edges) = graph.outgoing_edges(nid) {
            for edge in edges {
                targets.push(edge.dst_id);
            }
        }
        out_targets.insert(nid, targets);
    }

    let init = 1.0 / n as f64;
    let mut ranks: HashMap<u64, f64> = node_ids.iter().map(|&id| (id, init)).collect();

    for _ in 0..max_iterations {
        let mut next: HashMap<u64, f64> = node_ids.iter().map(|&id| (id, base)).collect();

        // 悬挂节点（出度为 0）的质量按全图均分再分配
        let mut dangling_mass = 0.0f64;
        for &nid in &node_ids {
            let targets = &out_targets[&nid];
            if targets.is_empty() {
                dangling_mass += ranks[&nid];
            }
        }
        let dangling_share = d * dangling_mass / n as f64;

        for &nid in &node_ids {
            let targets = &out_targets[&nid];
            if targets.is_empty() {
                continue;
            }
            let share = d * ranks[&nid] / targets.len() as f64;
            if share == 0.0 {
                continue;
            }
            for &dst in targets {
                if let Some(entry) = next.get_mut(&dst) {
                    *entry += share;
                }
            }
        }

        let mut delta = 0.0f64;
        for &nid in &node_ids {
            let updated = next[&nid] + dangling_share;
            delta += (updated - ranks[&nid]).abs();
            ranks.insert(nid, updated);
        }

        if delta < tolerance {
            break;
        }
    }

    let mut scores: Vec<PageRankScore> = node_ids
        .iter()
        .map(|&id| PageRankScore {
            node_id: id,
            score: ranks[&id],
        })
        .collect();
    scores.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    scores
}

/// 弱连通分量分析：把所有边视作无向连接，用并查集划分社群 / 检测孤岛。
///
/// 返回按分量规模降序排列的分量列表，每个分量内部节点 ID 升序。
pub fn weakly_connected_components(graph: &DiskGraph) -> Vec<Vec<u64>> {
    let node_ids = match graph.all_node_ids() {
        Ok(ids) => ids,
        Err(_) => return Vec::new(),
    };
    if node_ids.is_empty() {
        return Vec::new();
    }

    let mut parent: HashMap<u64, u64> = node_ids.iter().map(|&id| (id, id)).collect();

    fn find(parent: &mut HashMap<u64, u64>, mut x: u64) -> u64 {
        let mut root = x;
        while let Some(&p) = parent.get(&root) {
            if p == root {
                break;
            }
            root = p;
        }
        while let Some(&p) = parent.get(&x) {
            if p == root {
                break;
            }
            parent.insert(x, root);
            x = p;
        }
        root
    }

    for &nid in &node_ids {
        if let Ok(edges) = graph.outgoing_edges(nid) {
            for edge in edges {
                let ra = find(&mut parent, nid);
                let rb = find(&mut parent, edge.dst_id);
                if ra != rb {
                    parent.insert(rb, ra);
                }
            }
        }
    }

    let mut groups: HashMap<u64, Vec<u64>> = HashMap::new();
    for &nid in &node_ids {
        let root = find(&mut parent, nid);
        groups.entry(root).or_default().push(nid);
    }

    let mut components: Vec<Vec<u64>> = groups
        .into_values()
        .map(|mut v| {
            v.sort_unstable();
            v
        })
        .collect();
    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    components
}

/// K-Hop 局部子图提取结果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KHopSubgraph {
    /// 起始节点及其 K 步以内可达的全部节点（去重，升序）
    pub nodes: Vec<u64>,
    /// 子图内部的边集合
    pub edges: Vec<Edge>,
}

impl KHopSubgraph {
    /// 起始节点是否存在于子图中
    pub fn contains(&self, node_id: u64) -> bool {
        self.nodes.binary_search(&node_id).is_ok()
    }
}

/// 抽取指定节点 K 步以内的局部子图（BFS 逐层扩展，节点严格去重）。
///
/// - `direction`：邻接扩展方向（Outgoing / Incoming / Both）；
/// - `edge_type_filter`：可选关系类型过滤，`None` 表示不过滤。
pub fn k_hop_subgraph(
    graph: &DiskGraph,
    start_id: u64,
    k: usize,
    direction: Direction,
    edge_type_filter: Option<&str>,
) -> Result<KHopSubgraph, GraphError> {
    if graph.read_node_record(start_id)?.is_none() {
        return Err(GraphError::NodeNotFound(start_id));
    }

    let mut visited: HashSet<u64> = HashSet::new();
    visited.insert(start_id);

    let mut internal_edges: Vec<Edge> = Vec::new();
    let mut seen_edges: HashSet<u64> = HashSet::new();

    let mut frontier: VecDeque<(u64, usize)> = VecDeque::new();
    frontier.push_back((start_id, 0));

    while let Some((current, depth)) = frontier.pop_front() {
        if depth >= k {
            continue;
        }

        let candidate_edges = match direction {
            Direction::Outgoing => graph.outgoing_edges(current)?,
            Direction::Incoming => graph.incoming_edges(current)?,
            Direction::Both => {
                let mut both = graph.outgoing_edges(current)?;
                both.extend(graph.incoming_edges(current)?);
                both
            }
        };

        for edge in candidate_edges {
            if let Some(expected) = edge_type_filter {
                if edge.edge_type != expected {
                    continue;
                }
            }

            let neighbor = if edge.src_id == current {
                edge.dst_id
            } else {
                edge.src_id
            };

            if seen_edges.insert(edge.id) {
                internal_edges.push(edge.clone());
            }

            if visited.insert(neighbor) {
                frontier.push_back((neighbor, depth + 1));
            }
        }
    }

    let mut nodes: Vec<u64> = visited.into_iter().collect();
    nodes.sort_unstable();

    // 只保留两端均落在子图内的边
    let node_set: HashSet<u64> = nodes.iter().copied().collect();
    internal_edges.retain(|e| node_set.contains(&e.src_id) && node_set.contains(&e.dst_id));
    internal_edges.sort_by_key(|e| e.id);

    Ok(KHopSubgraph {
        nodes,
        edges: internal_edges,
    })
}
