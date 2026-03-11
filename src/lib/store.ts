import { create } from "zustand";

export type PageId = "overview" | "profiles" | "chat" | "notifications" | "docs" | "settings";

interface AppStore {
  page: PageId;
  activeProfileId?: string;
  viewingProfileId?: string;
  selectedConversationId?: string;
  streamingConversationId?: string;
  streamingBuffer: Record<string, string>;
  waitingConversations: Record<string, boolean>;
  setPage: (page: PageId) => void;
  setActiveProfileId: (profileId?: string) => void;
  setViewingProfileId: (profileId?: string) => void;
  setSelectedConversationId: (conversationId?: string) => void;
  appendStreamingChunk: (conversationId: string, chunk: string) => void;
  setStreamingConversationId: (conversationId?: string) => void;
  setWaitingConversation: (conversationId: string, waiting: boolean) => void;
  clearStreamingConversation: (conversationId: string) => void;
}

export const useAppStore = create<AppStore>((set) => ({
  page: "overview",
  streamingBuffer: {},
  waitingConversations: {},
  setPage: (page) => set({ page }),
  setActiveProfileId: (activeProfileId) => set({ activeProfileId }),
  setViewingProfileId: (viewingProfileId) => set({ viewingProfileId }),
  setSelectedConversationId: (selectedConversationId) => set({ selectedConversationId }),
  appendStreamingChunk: (conversationId, chunk) =>
    set((state) => ({
      streamingConversationId: conversationId,
      waitingConversations: {
        ...state.waitingConversations,
        [conversationId]: false
      },
      streamingBuffer: {
        ...state.streamingBuffer,
        [conversationId]: `${state.streamingBuffer[conversationId] ?? ""}${chunk}`
      }
    })),
  setStreamingConversationId: (streamingConversationId) => set({ streamingConversationId }),
  setWaitingConversation: (conversationId, waiting) =>
    set((state) => ({
      waitingConversations: {
        ...state.waitingConversations,
        [conversationId]: waiting
      }
    })),
  clearStreamingConversation: (conversationId) =>
    set((state) => {
      const next = { ...state.streamingBuffer };
      const waiting = { ...state.waitingConversations };
      delete next[conversationId];
      delete waiting[conversationId];
      return {
        streamingConversationId:
          state.streamingConversationId === conversationId
            ? undefined
            : state.streamingConversationId,
        waitingConversations: waiting,
        streamingBuffer: next
      };
    })
}));
