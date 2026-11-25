// What the screens share: an artifact's bytes shown by a `blob:` address (an `<img>`
// sends no token, so the page fetches the bytes with it), and a clock that ticks.
import { useEffect, useState } from "preact/hooks";
import { artifactPath, type Client } from "./api";

/** How many pictures one page of the library asks for (a field's, and search's). */
export const PICTURES_PAGE = 48;

/** The address the page shows an artifact from, once its bytes are here. */
export function useArtifact(client: Client, id: string | null | undefined, edge?: number): string | undefined {
  return useArtifactState(client, id, edge).url;
}

/**
 * The same, and why the bytes could not be had when they could not (a picture the host
 * was too busy for every time it was asked, or one the store does not hold), so a screen
 * says so rather than "Loading" for ever.
 */
export function useArtifactState(client: Client, id: string | null | undefined, edge?: number): { url?: string; failed?: string } {
  const [url, setUrl] = useState<string | undefined>(undefined);
  const [failed, setFailed] = useState<string | undefined>(undefined);
  useEffect(() => {
    setUrl(undefined);
    setFailed(undefined);
    if (!id) return;
    let live = true;
    let path: string;
    try {
      path = artifactPath(id, edge);
    } catch {
      return;
    }
    // Given up when the screen that asked goes: a fetch nobody waits for is aborted, so it
    // holds none of the two places the page fetches artifacts in.
    const asking = new AbortController();
    client.blobUrl(path, asking.signal).then(
      (made) => live && setUrl(made),
      (error: unknown) => live && setFailed(error instanceof Error ? error.message : String(error)),
    );
    return () => {
      live = false;
      asking.abort();
    };
  }, [id, edge]);
  return { url, failed };
}

/** The time now, again every `ms` while `on`. */
export function useNow(on: boolean, ms = 250): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!on) return;
    const timer = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(timer);
  }, [on, ms]);
  return now;
}

/** A picture's size in pixels, once the page has loaded it from its address. */
export function useImageSize(url: string | undefined): { width: number; height: number } | null {
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  useEffect(() => {
    setSize(null);
    if (!url || typeof Image === "undefined") return;
    let live = true;
    const image = new Image();
    image.onload = () => live && image.naturalWidth > 0 && setSize({ width: image.naturalWidth, height: image.naturalHeight });
    image.src = url;
    return () => {
      live = false;
    };
  }, [url]);
  return size;
}
