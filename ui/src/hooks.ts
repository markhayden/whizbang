// What the screens share: an artifact's bytes shown by a `blob:` address (an `<img>`
// sends no token, so the page fetches the bytes with it), and a clock that ticks.
import { useEffect, useState } from "preact/hooks";
import { artifactPath, type Client } from "./api";

/** The address the page shows an artifact from, once its bytes are here. */
export function useArtifact(client: Client, id: string | null | undefined, edge?: number): string | undefined {
  const [url, setUrl] = useState<string | undefined>(undefined);
  useEffect(() => {
    setUrl(undefined);
    if (!id) return;
    let live = true;
    let path: string;
    try {
      path = artifactPath(id, edge);
    } catch {
      return;
    }
    client.blobUrl(path).then(
      (made) => live && setUrl(made),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [id, edge]);
  return url;
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
