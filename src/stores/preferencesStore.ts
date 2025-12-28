import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface PreferencesState {
  // Audio
  isSoundEnabled: boolean;

  // Filters
  minSize: number;
  selectedSymbol: string;

  // Notifications
  notificationsEnabled: boolean;

  // Actions
  setSound: (enabled: boolean) => void;
  setMinSize: (size: number) => void;
  setSymbol: (symbol: string) => void;
  setNotifications: (enabled: boolean) => void;
  toggleSound: () => void;
  toggleNotifications: () => void;
}

export const usePreferencesStore = create<PreferencesState>()(
  persist(
    (set) => ({
      // Initial state
      isSoundEnabled: true,
      minSize: 10,
      selectedSymbol: 'all',
      notificationsEnabled: false,

      // Actions
      setSound: (enabled) => set({ isSoundEnabled: enabled }),
      setMinSize: (size) => set({ minSize: size }),
      setSymbol: (symbol) => set({ selectedSymbol: symbol }),
      setNotifications: (enabled) => set({ notificationsEnabled: enabled }),
      toggleSound: () => set((state) => ({ isSoundEnabled: !state.isSoundEnabled })),
      toggleNotifications: () => set((state) => ({ notificationsEnabled: !state.notificationsEnabled })),
    }),
    {
      name: 'hitthebid-preferences',
    }
  )
);
