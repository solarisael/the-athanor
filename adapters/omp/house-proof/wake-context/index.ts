import { catchBoat } from "../substrate.ts";

const AUTOMATIC_WAKE_IO_TIMEOUT_MS = 15_000;

type BoatReader = (
  room: string,
  options: { timeoutMs: number },
) => Promise<Record<string, unknown>>;

export async function receiveAutomaticWake(
  room: string,
  readBoat: BoatReader = catchBoat,
) {
  try {
    const boat = await readBoat(room, { timeoutMs: AUTOMATIC_WAKE_IO_TIMEOUT_MS });
    const answered = boat?.ok === true;
    if (!answered) {
      const code = String(boat?.code || "").trim();
      return {
        answered: false,
        letter: "",
        title: null,
        source: null,
        memoryId: null,
        warning: `paper boat unavailable${code ? ` (${code})` : ""}`,
      };
    }
    return {
      answered: true,
      letter: boat?.found ? String(boat.wake_context || "") : "",
      title: boat?.found && boat.title ? String(boat.title) : null,
      source: boat?.found && boat.source_path ? String(boat.source_path) : null,
      memoryId: boat?.found && boat.id ? String(boat.id) : null,
      warning: null,
    };
  } catch {
    return {
      answered: false,
      letter: "",
      title: null,
      source: null,
      memoryId: null,
      warning: "paper boat unavailable",
    };
  }
}
