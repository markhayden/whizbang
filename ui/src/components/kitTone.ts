/**
 * A kit's colour, by its place in the description's order of kits: rascal's copper and
 * squint's volt first, as the design draws them, then the other accents. The colour is
 * not the kit's own: a third kit takes the third tone with no change to the interface
 * (`SPEC.md` section 1: a door needs no change when a kit is added).
 */
export type KitTone = "copper" | "volt" | "unsure" | "pass" | "brass";

const TONES: KitTone[] = ["copper", "volt", "unsure", "pass", "brass"];

export function kitTone(index: number): KitTone {
  const at = ((index % TONES.length) + TONES.length) % TONES.length;
  return TONES[at] ?? "copper";
}
