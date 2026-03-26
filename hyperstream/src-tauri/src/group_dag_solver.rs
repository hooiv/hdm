/// Advanced Directed Acyclic Graph (DAG) Solver for Download Groups
///
/// Handles complex dependency resolution for download groups, enabling sophisticated
/// workflows like:
/// - A → B → C (sequential chains)
/// - A → [B1, B2, B3] (fan-out)
/// - [A1, A2] → B (fan-in)
/// - Diamond patterns: A → [B, C] → D
///
/// Detects cycles, computes critical path, and provides optimal execution ordering.

use crate::download_groups::DownloadGroup;
#[cfg(test)]
use crate::download_groups::{GroupMember, GroupState};
use std::collections::{HashMap, HashSet, VecDeque};

/// Topological sort result
#[derive(Debug, Clone)]
pub struct TopologicalOrder {
    /// List of member IDs in valid execution order
    pub order: Vec<String>,
    /// Depth of each member (0 = no deps, 1 = depends on depth-0, etc.)
    pub depths: HashMap<String, usize>,
    /// Critical path length (longest chain)
    pub critical_path_length: usize,
}

/// Cycle detection result
#[derive(Debug, Clone)]
pub struct CycleInfo {
    /// Whether a cycle exists
    pub has_cycle: bool,
    /// Members involved in the cycle (if any)
    pub cycle_members: Vec<String>,
}

/// Dependency path information
#[derive(Debug, Clone)]
pub struct DependencyPath {
    /// Path from start to target member
    pub path: Vec<String>,
    /// Total path length (number of hops)
    pub length: usize,
    /// Whether this is a critical path
    pub is_critical: bool,
}

/// Advanced DAG solver for group dependencies
pub struct DagSolver;

impl DagSolver {
    /// Perform topological sort on group members
    /// Returns members in valid execution order (respecting all dependencies)
    pub fn topological_sort(group: &DownloadGroup) -> Result<TopologicalOrder, String> {
        // First check for cycles
        let cycle_info = Self::detect_cycles(group);
        if cycle_info.has_cycle {
            return Err(format!(
                "Circular dependency detected: {:?}",
                cycle_info.cycle_members
            ));
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize in-degree and adjacency list
        for (id, member) in &group.members {
            in_degree.insert(id.clone(), member.dependencies.len());
            adj_list.insert(id.clone(), Vec::new());
        }

        // Build adjacency list (reverse dependencies)
        for (id, member) in &group.members {
            for dep in &member.dependencies {
                if let Some(neighbors) = adj_list.get_mut(dep) {
                    neighbors.push(id.clone());
                }
            }
        }

        // Kahn's algorithm for topological sort
        let mut queue: VecDeque<String> = VecDeque::new();
        for (id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(id.clone());
            }
        }

        let mut order = Vec::new();
        let mut depths: HashMap<String, usize> = HashMap::new();

        while let Some(node) = queue.pop_front() {
            order.push(node.clone());

            // Compute depth
            let max_dep_depth = group.members
                .get(&node)
                .map(|m| {
                    m.dependencies
                        .iter()
                        .filter_map(|dep| depths.get(dep))
                        .max()
                        .copied()
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            depths.insert(node.clone(), max_dep_depth + 1);

            if let Some(neighbors) = adj_list.get(&node) {
                for neighbor in neighbors {
                    in_degree.insert(
                        neighbor.clone(),
                        in_degree.get(neighbor).copied().unwrap_or(1) - 1,
                    );
                    if in_degree.get(neighbor).copied().unwrap_or(0) == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        if order.len() != group.members.len() {
            return Err("Circular dependency detected".to_string());
        }

        let critical_path_length = *depths.values().max().unwrap_or(&0);

        Ok(TopologicalOrder {
            order,
            depths,
            critical_path_length,
        })
    }

    /// Detect cycles using DFS (Depth-First Search)
    pub fn detect_cycles(group: &DownloadGroup) -> CycleInfo {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycle_members = Vec::new();

        for (id, _) in &group.members {
            if !visited.contains(id) {
                if Self::has_cycle_dfs(
                    id,
                    group,
                    &mut visited,
                    &mut rec_stack,
                    &mut cycle_members,
                ) {
                    return CycleInfo {
                        has_cycle: true,
                        cycle_members,
                    };
                }
            }
        }

        CycleInfo {
            has_cycle: false,
            cycle_members: Vec::new(),
        }
    }

    /// DFS helper for cycle detection
    fn has_cycle_dfs(
        node_id: &str,
        group: &DownloadGroup,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        cycle_members: &mut Vec<String>,
    ) -> bool {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());

        if let Some(member) = group.members.get(node_id) {
            for dep in &member.dependencies {
                if !visited.contains(dep) {
                    if Self::has_cycle_dfs(dep, group, visited, rec_stack, cycle_members) {
                        cycle_members.push(node_id.to_string());
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    cycle_members.push(node_id.to_string());
                    cycle_members.push(dep.clone());
                    return true;
                }
            }
        }

        rec_stack.remove(node_id);
        false
    }

    /// Find the critical path (longest chain of dependencies)
    pub fn find_critical_path(group: &DownloadGroup) -> Result<DependencyPath, String> {
        let topo = Self::topological_sort(group)?;

        // Find member(s) with maximum depth
        let max_depth = *topo.depths.values().max().unwrap_or(&0);
        let mut final_node = None;

        for (id, &depth) in &topo.depths {
            if depth == max_depth {
                final_node = Some(id.clone());
                break;
            }
        }

        if let Some(final_id) = final_node {
            // Reconstruct path backward
            let path = Self::extract_path(&final_id, group, &topo);
            Ok(DependencyPath {
                length: path.len(),
                is_critical: true,
                path,
            })
        } else {
            Err("Could not find critical path".to_string())
        }
    }

    /// Extract a path to a member by following dependencies
    fn extract_path(target: &str, group: &DownloadGroup, topo: &TopologicalOrder) -> Vec<String> {
        let mut path = vec![target.to_string()];
        let mut current = target.to_string();

        loop {
            if let Some(member) = group.members.get(&current) {
                if member.dependencies.is_empty() {
                    break;
                }

                // Pick the dependency with max depth (on critical path)
                if let Some(next) = member.dependencies.iter().max_by_key(|dep| {
                    topo.depths.get(*dep).copied().unwrap_or(0)
                }) {
                    path.insert(0, next.clone());
                    current = next.clone();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        path
    }

    /// Get all members that can execute in parallel at a given depth level
    pub fn get_parallel_batch(
        group: &DownloadGroup,
        depth: usize,
    ) -> Result<Vec<String>, String> {
        let topo = Self::topological_sort(group)?;
        Ok(topo
            .depths
            .iter()
            .filter(|(_, &d)| d == depth)
            .map(|(id, _)| id.clone())
            .collect())
    }

    /// Get execution plan optimized for strategy
    pub fn generate_execution_plan(
        group: &DownloadGroup,
    ) -> Result<Vec<Vec<String>>, String> {
        let topo = Self::topological_sort(group)?;
        let max_depth = *topo.depths.values().max().unwrap_or(&0);

        let mut plan = vec![Vec::new(); max_depth + 1];

        for (id, &depth) in &topo.depths {
            plan[depth].push(id.clone());
        }

        Ok(plan)
    }

    /// Validate that a group's dependencies are sane
    pub fn validate_dependencies(group: &DownloadGroup) -> Result<(), String> {
        // Check 1: No cycles
        let cycle_info = Self::detect_cycles(group);
        if cycle_info.has_cycle {
            return Err(format!(
                "Circular dependency: {:?}",
                cycle_info.cycle_members
            ));
        }

        // Check 2: All referenced dependencies exist
        for (_, member) in &group.members {
            for dep in &member.dependencies {
                if !group.members.contains_key(dep) {
                    return Err(format!(
                        "Dependency '{}' referenced by '{}' does not exist",
                        dep, member.id
                    ));
                }
            }
        }

        // Check 3: Dependencies are reasonable (max 10 levels deep)
        let topo = Self::topological_sort(group)?;
        if topo.critical_path_length > 10 {
            return Err(format!(
                "Dependency chain too deep ({}): Consider splitting into subgroups",
                topo.critical_path_length
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_chain() {
        let mut group = DownloadGroup::new("test");

        let m1 = GroupMember {
            id: "1".to_string(),
            url: "http://example.com/a".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec![],
        };

        let m2 = GroupMember {
            id: "2".to_string(),
            url: "http://example.com/b".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["1".to_string()],
        };

        let m3 = GroupMember {
            id: "3".to_string(),
            url: "http://example.com/c".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["2".to_string()],
        };

        group.members.insert("1".to_string(), m1);
        group.members.insert("2".to_string(), m2);
        group.members.insert("3".to_string(), m3);

        let topo = DagSolver::topological_sort(&group).unwrap();
        assert_eq!(topo.order, vec!["1", "2", "3"]);
        assert_eq!(topo.critical_path_length, 3);
    }

    #[test]
    fn test_cycle_detection() {
        let mut group = DownloadGroup::new("test");

        let m1 = GroupMember {
            id: "1".to_string(),
            url: "http://example.com/a".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["2".to_string()],
        };

        let m2 = GroupMember {
            id: "2".to_string(),
            url: "http://example.com/b".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["1".to_string()],
        };

        group.members.insert("1".to_string(), m1);
        group.members.insert("2".to_string(), m2);

        let cycle_info = DagSolver::detect_cycles(&group);
        assert!(cycle_info.has_cycle);
    }

    #[test]
    fn test_fan_out_fan_in() {
        let mut group = DownloadGroup::new("test");

        // A → [B, C, D] → E
        let a = GroupMember {
            id: "A".to_string(),
            url: "http://example.com/a".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec![],
        };

        let b = GroupMember {
            id: "B".to_string(),
            url: "http://example.com/b".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["A".to_string()],
        };

        let c = GroupMember {
            id: "C".to_string(),
            url: "http://example.com/c".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["A".to_string()],
        };

        let d = GroupMember {
            id: "D".to_string(),
            url: "http://example.com/d".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["A".to_string()],
        };

        let e = GroupMember {
            id: "E".to_string(),
            url: "http://example.com/e".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["B".to_string(), "C".to_string(), "D".to_string()],
        };

        group.members.insert("A".to_string(), a);
        group.members.insert("B".to_string(), b);
        group.members.insert("C".to_string(), c);
        group.members.insert("D".to_string(), d);
        group.members.insert("E".to_string(), e);

        let topo = DagSolver::topological_sort(&group).unwrap();
        assert_eq!(topo.critical_path_length, 3);

        let plan = DagSolver::generate_execution_plan(&group).unwrap();
        assert_eq!(plan[0].len(), 1); // A only
        assert_eq!(plan[1].len(), 3); // B, C, D can run parallel
        assert_eq!(plan[2].len(), 1); // E only
    }

    #[test]
    fn test_validation() {
        let mut group = DownloadGroup::new("test");

        let m1 = GroupMember {
            id: "1".to_string(),
            url: "http://example.com/a".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec!["nonexistent".to_string()],
        };

        group.members.insert("1".to_string(), m1);

        let result = DagSolver::validate_dependencies(&group);
        assert!(result.is_err());
    }
}
