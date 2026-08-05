import { forceQuitProfile, quitProfile } from "../api";
import { showQuitStuck } from "./errors";

/** Resolves true when the profile is down, false when the user abandoned the operation. */
export async function quitRunning(id: string, name: string): Promise<boolean> {
  try {
    await quitProfile(id);
    return true;
  } catch {
    return new Promise((resolve) => {
      showQuitStuck(
        name,
        () => void forceQuitProfile(id).then(() => resolve(true)).catch(() => resolve(false)),
        () => resolve(false),
      );
    });
  }
}
