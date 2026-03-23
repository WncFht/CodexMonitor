import { useMemo } from "react";
import type { ConversationItem } from "@/types";
import { enrichConversationItemsWithThreads } from "@utils/threadItems";
import type { ThreadState } from "./useThreadsReducer";

type UseThreadSelectorsOptions = {
  activeWorkspaceId: string | null;
  activeThreadIdByWorkspace: ThreadState["activeThreadIdByWorkspace"];
  itemsByThread: ThreadState["itemsByThread"];
  threadsByWorkspace: ThreadState["threadsByWorkspace"];
};

export function useThreadSelectors({
  activeWorkspaceId,
  activeThreadIdByWorkspace,
  itemsByThread,
  threadsByWorkspace,
}: UseThreadSelectorsOptions) {
  const activeThreadId = useMemo(() => {
    if (!activeWorkspaceId) {
      return null;
    }
    return activeThreadIdByWorkspace[activeWorkspaceId] ?? null;
  }, [activeThreadIdByWorkspace, activeWorkspaceId]);

  const activeWorkspaceThreads = activeWorkspaceId
    ? threadsByWorkspace[activeWorkspaceId]
    : undefined;

  const activeItems = useMemo<ConversationItem[]>(
    () => {
      if (!activeThreadId) {
        return [];
      }
      const items = itemsByThread[activeThreadId] ?? [];
      const threads = activeWorkspaceThreads ?? [];
      return enrichConversationItemsWithThreads(items, threads);
    },
    [activeThreadId, activeWorkspaceThreads, itemsByThread],
  );

  return { activeThreadId, activeItems };
}
