import {
  HostUnavailable,
  hostCommand,
  sendHostCommand,
  type HostBinding,
  type HostResponse,
} from "./host.ts";

const HALLWAY_INBOX_PROJECT = "athanor.hallway.inbox_project";
const HALLWAY_INBOX_PROJECTED = "athanor.hallway.inbox_projected";

type HallwayInbox = {
  ok: boolean;
  hallways: Array<Record<string, any>>;
};

export type HallwayInboxProjection = {
  changed: boolean;
  inbox: HallwayInbox;
};

export async function projectHallwayInbox(
  binding: HostBinding,
  signal?: AbortSignal,
): Promise<HallwayInboxProjection> {
  const response = await sendHostCommand(
    hostCommand(binding, HALLWAY_INBOX_PROJECT, "hallway"),
    new Set([HALLWAY_INBOX_PROJECTED]),
    signal,
  ) as HostResponse & { changed?: unknown; inbox?: unknown };
  const inbox = response.inbox;
  if (!inbox || typeof inbox !== "object" || Array.isArray(inbox)) {
    throw new HostUnavailable("Hallway Host projection omitted the inbox");
  }
  const hallways = (inbox as Record<string, unknown>).hallways;
  if (!Array.isArray(hallways)) {
    throw new HostUnavailable("Hallway Host projection omitted hallway rows");
  }
  return {
    changed: response.changed === true,
    inbox: inbox as HallwayInbox,
  };
}
