use crate::disk_graph::DiskGraph;
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
            && dfs_has_cycle(graph, node_id, &mut color_map)
        {
            return true;
        }
    }

    false
}

fn dfs_has_cycle(graph: &DiskGraph, node_id: u64, color_map: &mut HashMap<u64, Color>) -> bool {
    color_map.insert(node_id, Color::Gray);

    if let Ok(neighbors) = graph.neighbors(node_id, crate::graph::Direction::Outgoing) {
        for neighbor in neighbors {
            match color_map.get(&neighbor).copied() {
                Some(Color::Gray) => return true,
                Some(Color::White) if dfs_has_cycle(graph, neighbor, color_map) => {
                    return true;
                }
                _ => {}
            }
        }
    }

    color_map.insert(node_id, Color::Black);
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

    let mut path_stack = Vec::new();
    let mut cycles = Vec::new();

    for &node_id in &node_ids {
        if color_map.get(&node_id) == Some(&Color::White) {
            dfs_find_cycles(graph, node_id, &mut color_map, &mut path_stack, &mut cycles);
        }
    }

    cycles
}

fn dfs_find_cycles(
    graph: &DiskGraph,
    node_id: u64,
    color_map: &mut HashMap<u64, Color>,
    path_stack: &mut Vec<u64>,
    cycles: &mut Vec<Vec<u64>>,
) {
    color_map.insert(node_id, Color::Gray);
    path_stack.push(node_id);

    if let Ok(neighbors) = graph.neighbors(node_id, crate::graph::Direction::Outgoing) {
        for neighbor in neighbors {
            match color_map.get(&neighbor) {
                Some(Color::Gray) => {
                    if let Some(pos) = path_stack.iter().position(|&x| x == neighbor) {
                        let mut cycle = path_stack[pos..].to_vec();
                        cycle.push(neighbor);
                        cycles.push(cycle);
                    }
                }
                Some(Color::White) => {
                    dfs_find_cycles(graph, neighbor, color_map, path_stack, cycles);
                }
                _ => {}
            }
        }
    }

    path_stack.pop();
    color_map.insert(node_id, Color::Black);
}
