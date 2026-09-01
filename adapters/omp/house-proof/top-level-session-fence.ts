const topLevelSessions = new Map<string, string>();

export function registerTopLevelSession(room: string, session: string): void {
  const roomKey = text(room);
  const sessionKey = text(session);
  if (!roomKey || !sessionKey) return;
  topLevelSessions.set(roomKey, sessionKey);
}

// session_start also fires for workers. First-wins keeps a worker from
// displacing a spirit that already holds the room.
export function adoptTopLevelSession(room: string, session: string): void {
  const roomKey = text(room);
  if (!roomKey || topLevelSessions.has(roomKey)) return;
  registerTopLevelSession(roomKey, session);
}

// A worker shutdown cannot vacate the authenticated top-level session.
export function retireTopLevelSession(room: string, session: string): void {
  const roomKey = text(room);
  if (roomKey && topLevelSessions.get(roomKey) === text(session)) {
    topLevelSessions.delete(roomKey);
  }
}

export function topLevelSession(room: string): string | null {
  return topLevelSessions.get(text(room)) ?? null;
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}
