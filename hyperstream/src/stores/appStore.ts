import { create } from 'zustand';
import type { DownloadTask, AppSettings } from '../types';

/**
 * Production-grade state store for HyperStream
 * 
 * Separates concerns:
 * - Download progress updates (high-frequency, ~30fps)
 * - Modal states (low-frequency, user interactions)
 * - Settings (rare, user changes)
 * - UI state (navigation, etc.)
 * 
 * This prevents full app re-renders on every 30ms progress update.
 */

// ============ DOWNLOAD STATE STORE ============
interface DownloadState {
  tasks: Map<string, DownloadTask>;
  addTask: (task: DownloadTask) => void;
  updateTaskProgress: (taskId: string, updates: Partial<DownloadTask>) => void;
  removeTask: (taskId: string) => void;
  setTasks: (tasks: DownloadTask[]) => void;
  getTasks: () => DownloadTask[];
}

export const useDownloadStore = create<DownloadState>((set, get) => ({
  tasks: new Map(),
  
  addTask: (task) => {
    set((state) => {
      const newMap = new Map(state.tasks);
      newMap.set(task.id, task);
      return { tasks: newMap };
    });
  },

  updateTaskProgress: (taskId, updates) => {
    set((state) => {
      const task = state.tasks.get(taskId);
      if (!task) return state;
      
      const newMap = new Map(state.tasks);
      newMap.set(taskId, { ...task, ...updates });
      return { tasks: newMap };
    });
  },

  removeTask: (taskId) => {
    set((state) => {
      const newMap = new Map(state.tasks);
      newMap.delete(taskId);
      return { tasks: newMap };
    });
  },

  setTasks: (tasks) => {
    const map = new Map(tasks.map(t => [t.id, t]));
    set({ tasks: map });
  },

  getTasks: () => {
    return Array.from(get().tasks.values());
  },
}));

// ============ MODAL STATE STORE ============
interface ModalState {
  modalsOpen: Map<string, boolean>;
  openModal: (modalName: string) => void;
  closeModal: (modalName: string) => void;
  isModalOpen: (modalName: string) => boolean;
}

export const useModalStore = create<ModalState>((set, get) => ({
  modalsOpen: new Map(),

  openModal: (modalName) => {
    set((state) => {
      const newMap = new Map(state.modalsOpen);
      newMap.set(modalName, true);
      return { modalsOpen: newMap };
    });
  },

  closeModal: (modalName) => {
    set((state) => {
      const newMap = new Map(state.modalsOpen);
      newMap.set(modalName, false);
      return { modalsOpen: newMap };
    });
  },

  isModalOpen: (modalName) => {
    return get().modalsOpen.get(modalName) ?? false;
  },
}));

// ============ SETTINGS STATE STORE ============
interface SettingsState {
  settings: AppSettings | null;
  isLoading: boolean;
  error: string | null;
  setSettings: (settings: AppSettings) => void;
  updateSetting: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: null,
  isLoading: false,
  error: null,

  setSettings: (settings) => set({ settings, error: null }),
  
  updateSetting: (key, value) => {
    set((state) => ({
      settings: state.settings ? { ...state.settings, [key]: value } : null,
    }));
  },

  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),
}));

// ============ UI STATE STORE ============
interface UIState {
  activeTab: string;
  sidebarOpen: boolean;
  selectedTaskId: string | null;
  setActiveTab: (tab: string) => void;
  toggleSidebar: () => void;
  selectTask: (taskId: string | null) => void;
}

export const useUIStore = create<UIState>((set) => ({
  activeTab: 'downloads',
  sidebarOpen: true,
  selectedTaskId: null,

  setActiveTab: (tab) => set({ activeTab: tab }),
  toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
  selectTask: (taskId) => set({ selectedTaskId: taskId }),
}));

// ============ NOTIFICATION/TOAST STATE STORE ============
export interface Toast {
  id: string;
  message: string;
  type: 'info' | 'success' | 'warning' | 'error';
  duration?: number;
}

interface ToastState {
  toasts: Toast[];
  addToast: (toast: Omit<Toast, 'id'>) => void;
  removeToast: (id: string) => void;
  clearAllToasts: () => void;
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],

  addToast: (toast) => {
    const id = `toast_${Date.now()}_${Math.random()}`;
    set((state) => ({
      toasts: [...state.toasts, { ...toast, id }],
    }));

    // Auto-remove after duration
    if (toast.duration) {
      setTimeout(() => {
        set((state) => ({
          toasts: state.toasts.filter((t) => t.id !== id),
        }));
      }, toast.duration);
    }
  },

  removeToast: (id) => {
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },

  clearAllToasts: () => set({ toasts: [] }),
}));

// ============ DOWNLOAD GROUPS STATE STORE ============
export interface DownloadGroup {
  id: string;
  name: string;
  state: 'pending' | 'downloading' | 'paused' | 'completed' | 'error';
  members: Array<{
    id: string;
    url: string;
    progress: number;
    state: string;
  }>;
  createdAt: number;
  completedAt?: number;
}

interface GroupsState {
  groups: Map<string, DownloadGroup>;
  addGroup: (group: DownloadGroup) => void;
  updateGroup: (groupId: string, updates: Partial<DownloadGroup>) => void;
  removeGroup: (groupId: string) => void;
  setGroups: (groups: DownloadGroup[]) => void;
  getGroups: () => DownloadGroup[];
}

export const useGroupsStore = create<GroupsState>((set, get) => ({
  groups: new Map(),

  addGroup: (group) => {
    set((state) => {
      const newMap = new Map(state.groups);
      newMap.set(group.id, group);
      return { groups: newMap };
    });
  },

  updateGroup: (groupId, updates) => {
    set((state) => {
      const group = state.groups.get(groupId);
      if (!group) return state;

      const newMap = new Map(state.groups);
      newMap.set(groupId, { ...group, ...updates });
      return { groups: newMap };
    });
  },

  removeGroup: (groupId) => {
    set((state) => {
      const newMap = new Map(state.groups);
      newMap.delete(groupId);
      return { groups: newMap };
    });
  },

  setGroups: (groups) => {
    const map = new Map(groups.map((g) => [g.id, g]));
    set({ groups: map });
  },

  getGroups: () => {
    return Array.from(get().groups.values());
  },
}));
