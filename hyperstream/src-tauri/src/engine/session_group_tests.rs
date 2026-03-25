// Group Integration Tests
#[cfg(test)]
mod group_integration_tests {
    use super::super::*;
    use crate::download_groups::{DownloadGroup, GroupState, ExecutionStrategy};
    use crate::group_scheduler::{GroupScheduler, GLOBAL_GROUP_SCHEDULER, ExecutionState};

    #[test]
    fn test_group_dependency_checking_blocks_unmet_dependencies() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Test Group");
        
        // Create two members with dependency
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", Some(vec![m1.clone()]));
        
        // m2 has unmet dependency on m1
        assert!(!scheduler.can_start_member(&group.id, &m2));
        
        // m1 should be ready (no dependencies)
        assert!(scheduler.can_start_member(&group.id, &m1));
        
        // Schedule the group
        assert!(scheduler.schedule_group(group.clone()).is_ok());
        
        // Verify readiness check works
        assert!(!scheduler.can_start_member(&group.id, &m2));
        assert!(scheduler.can_start_member(&group.id, &m1));
    }

    #[test]
    fn test_group_progress_clamping() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Progress Test");
        let member_id = group.add_member("http://example.com/file.txt", None);
        
        scheduler.schedule_group(group).unwrap();
        
        // Update progress above 100% - should clamp
        scheduler.update_member_progress(&group.id, &member_id, 150.0);
        
        if let Some(group) = scheduler.get_group(&group.id) {
            if let Some(member) = group.members.get(&member_id) {
                assert_eq!(member.progress_percent, 100.0);
                assert_eq!(member.state, GroupState::Completed);
            }
        }
        
        // Test negative progress - should clamp to 0
        let mut scheduler2 = GroupScheduler::new();
        let mut group2 = DownloadGroup::new("Progress Test 2");
        let member_id2 = group2.add_member("http://example.com/file2.txt", None);
        scheduler2.schedule_group(group2).unwrap();
        
        scheduler2.update_member_progress(&group.id, &member_id2, -50.0);
        
        if let Some(group) = scheduler2.get_group(&group.id) {
            if let Some(member) = group.members.get(&member_id2) {
                assert_eq!(member.progress_percent, 0.0);
            }
        }
    }

    #[test]
    fn test_member_completion_auto_completes_group() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Single Member Group");
        let member_id = group.add_member("http://example.com/file.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Initially, group should not be complete
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete the member
        assert!(scheduler.complete_member(&group_id, &member_id).is_ok());
        
        // Now group should be complete
        assert!(scheduler.is_group_complete(&group_id));
        
        // Group state should be Completed
        if let Some(group) = scheduler.get_group(&group_id) {
            assert_eq!(group.state, GroupState::Completed);
        }
    }

    #[test]
    fn test_multiple_members_group_partial_completion() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Multi Member Group");
        
        // Add three independent members
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let m3 = group.add_member("http://example.com/file3.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Complete first member
        scheduler.complete_member(&group_id, &m1).unwrap();
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete second member
        scheduler.complete_member(&group_id, &m2).unwrap();
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete third member
        scheduler.complete_member(&group_id, &m3).unwrap();
        assert!(scheduler.is_group_complete(&group_id));
    }

    #[test]
    fn test_failure_in_one_member_doesnt_affect_others() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Failure Test Group");
        
        // Add two independent members
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Mark first member as failed
        scheduler.fail_member(&group_id, &m1, "Network error").unwrap();
        
        // First member should be in Error state
        if let Some(group) = scheduler.get_group(&group_id) {
            assert_eq!(group.members[&m1].state, GroupState::Error);
            // Second member should still be Pending
            assert_eq!(group.members[&m2].state, GroupState::Pending);
        }
        
        // Group should be in Error state but second member should still be startable
        assert!(scheduler.can_start_member(&group_id, &m2));
    }

    #[test]
    fn test_dependency_ordering_sequential() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Sequential Group");
        group.strategy = ExecutionStrategy::Sequential;
        
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", Some(vec![m1.clone()]));
        let m3 = group.add_member("http://example.com/file3.txt", Some(vec![m2.clone()]));
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Only m1 should be ready initially
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m1.clone()]);
        
        // Complete m1
        scheduler.complete_member(&group_id, &m1).unwrap();
        
        // Now m2 should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m2.clone()]);
        
        // Complete m2
        scheduler.complete_member(&group_id, &m2).unwrap();
        
        // Now m3 should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m3.clone()]);
    }

    #[test]
    fn test_group_member_progress_tracking() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Progress Tracking Group");
        let member_id = group.add_member("http://example.com/file.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Update progress incrementally
        let progress_steps = vec![10.0, 25.0, 50.0, 75.0, 99.5, 100.0];
        
        for progress in progress_steps {
            scheduler.update_member_progress(&group_id, &member_id, progress);
            
            if let Some(group) = scheduler.get_group(&group_id) {
                if let Some(member) = group.members.get(&member_id) {
                    if progress < 100.0 {
                        assert_eq!(member.state, GroupState::Pending);
                    } else {
                        assert_eq!(member.state, GroupState::Completed);
                    }
                    assert!((member.progress_percent - progress.clamp(0.0, 100.0)).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn test_group_with_complex_dependencies() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Complex Dependencies");
        
        // Create a diamond dependency pattern:
        //     A (root)
        //    / \
        //   B   C
        //    \ /
        //     D
        let a = group.add_member("http://example.com/a.txt", None);
        let b = group.add_member("http://example.com/b.txt", Some(vec![a.clone()]));
        let c = group.add_member("http://example.com/c.txt", Some(vec![a.clone()]));
        let d = group.add_member("http://example.com/d.txt", Some(vec![b.clone(), c.clone()]));
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Only A should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![a.clone()]);
        
        // Complete A
        scheduler.complete_member(&group_id, &a).unwrap();
        
        // Now B and C should be ready
        let mut ready = scheduler.get_ready_members(&group_id);
        ready.sort();
        let mut expected = vec![b.clone(), c.clone()];
        expected.sort();
        assert_eq!(ready, expected);
        
        // Complete B and C
        scheduler.complete_member(&group_id, &b).unwrap();
        scheduler.complete_member(&group_id, &c).unwrap();
        
        // Now D should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![d.clone()]);
    }

    #[test]
    fn test_get_completed_and_pending_members() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Status Test Group");
        
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let m3 = group.add_member("http://example.com/file3.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // All should be pending initially
        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 3);
        assert!(scheduler.get_completed_members(&group_id).is_empty());
        
        // Complete m1
        scheduler.complete_member(&group_id, &m1).unwrap();
        
        // Should have 1 completed and 2 pending
        assert_eq!(scheduler.get_completed_members(&group_id).len(), 1);
        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 2);
    }
}
